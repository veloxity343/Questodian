//! Reading and writing records, the same way for every kind.
//!
//! Records travel as JSON in the shape of [`crate::schemas`]. Rows are converted using
//! the column types the database declares (see [`ColumnKind`]), so there is no
//! per-table mapping code. A [`Repo`] is one transaction: nothing is saved until
//! [`Repo::commit`], and dropping it rolls everything back.

use std::collections::HashMap;
use std::path::PathBuf;

use rusqlite::types::{ToSqlOutput, ValueRef};
use rusqlite::{Connection, OptionalExtension, Row, params, params_from_iter};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::builder::Override;
use crate::db::Pool;
use crate::error::{Error, Result, invalid, not_found};
use crate::kinds::{Column, ColumnKind, FILTERS, Kind, LINKS};

/// A new record id: 12 hex characters.
pub fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_string()
}

/// The current time in UTC, e.g. `2026-11-16T12:00:00+00:00`.
pub fn now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S+00:00").to_string()
}

/// Ids may be any 1-64 letters, digits, `_` or `-` (imports keep their ids).
pub fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// A record's title for file names: its `title`, else its `name`.
pub fn title_of(doc: &Value) -> String {
    let text = |key| doc.get(key).and_then(Value::as_str).filter(|s| !s.is_empty());
    text("title").or_else(|| text("name")).unwrap_or("").to_string()
}

/// A stored record that can't be read (e.g. edited by hand), so lists and exports leave it out.
#[derive(Debug, Clone, Serialize)]
pub struct Unreadable {
    #[serde(serialize_with = "plural")]
    pub kind: Kind,
    /// Empty if even the id can't be read.
    pub id: String,
    /// As stored, for campaign exports: empty if there's none, `None` if it can't be read.
    #[serde(skip)]
    pub campaign_id: Option<String>,
    pub error: String,
}

fn plural<S: serde::Serializer>(kind: &Kind, serializer: S) -> std::result::Result<S::Ok, S::Error> {
    serializer.serialize_str(kind.plural())
}

impl std::fmt::Display for Unreadable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let id = if self.id.is_empty() { "(no id)" } else { &self.id };
        write!(f, "{} {id} that can't be read: {}", self.kind.singular(), self.error)
    }
}

pub struct Repo<'a> {
    /// A connection from the pool, inside this unit of work's transaction.
    conn: Option<Connection>,
    pool: &'a Pool,
    /// The writers' turn, for a unit of work that writes; given up after the connection.
    _turn: Option<tokio::sync::MutexGuard<'a, ()>>,
    columns: &'a HashMap<Kind, Vec<Column>>,
    /// Where handout images live.
    pub files_dir: PathBuf,
    /// Files no record uses once this unit of work is saved (images of replaced handouts).
    remove_on_commit: Vec<PathBuf>,
    /// For a unit of work that writes: the id of the save being made, if it has one, to
    /// record with it (see [`crate::db::Database::saving`]).
    saving: Option<&'a std::sync::Mutex<Option<String>>>,
}

impl<'a> Repo<'a> {
    /// Start a unit of work with `begin` (`BEGIN`, or `BEGIN IMMEDIATE` to write).
    pub(crate) fn begin(
        pool: &'a Pool,
        begin: &str,
        turn: Option<tokio::sync::MutexGuard<'a, ()>>,
        saving: Option<&'a std::sync::Mutex<Option<String>>>,
        columns: &'a HashMap<Kind, Vec<Column>>,
        files_dir: PathBuf,
    ) -> Result<Self> {
        let conn = pool.get()?;
        if let Err(e) = conn.execute_batch(begin) {
            pool.put(conn);
            return Err(e.into());
        }
        Ok(Repo { conn: Some(conn), pool, _turn: turn, columns, files_dir, remove_on_commit: Vec::new(), saving })
    }

    pub(crate) fn conn(&self) -> &Connection {
        self.conn.as_ref().expect("a unit of work has its connection until it's dropped")
    }

    /// Save everything done in this unit of work, then remove the files it no longer uses.
    pub async fn commit(mut self) -> Result<()> {
        let id = self.saving.and_then(|s| s.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take());
        if let Some(id) = id {
            self.conn()
                .prepare_cached("INSERT OR REPLACE INTO saved_requests (id, saved) VALUES (?, ?)")?
                .execute(params![id, now()])?;
        }
        self.conn().execute_batch("COMMIT")?;
        for path in std::mem::take(&mut self.remove_on_commit) {
            crate::files::remove(&path);
        }
        Ok(())
    }

    /// Remove a file once this unit of work is saved (kept if it's rolled back).
    pub fn remove_on_commit(&mut self, path: PathBuf) {
        self.remove_on_commit.push(path);
    }

    fn columns(&self, kind: Kind) -> &'a [Column] {
        &self.columns[&kind]
    }

    /// Whether records of this kind have this field.
    pub(crate) fn has_column(&self, kind: Kind, name: &str) -> bool {
        self.columns(kind).iter().any(|c| c.name == name)
    }

    // ------------------------------------------------------------------ reads

    /// All records of a kind, optionally filtered (empty values are ignored). Records that
    /// can't be read are left out: use [`Repo::list_checked`] where that matters.
    pub async fn list(&mut self, kind: Kind, filters: &[(&str, &str)]) -> Result<Vec<Value>> {
        let (docs, unreadable) = self.list_checked(kind, filters).await?;
        for record in unreadable {
            eprintln!("questodian: skipping {record}");
        }
        Ok(docs)
    }

    /// All records of a kind, as [`Repo::list`], and the ones left out because they can't be
    /// read (for the caller to report).
    pub async fn list_checked(
        &mut self,
        kind: Kind,
        filters: &[(&str, &str)],
    ) -> Result<(Vec<Value>, Vec<Unreadable>)> {
        let columns = self.columns(kind);
        let mut conditions = Vec::new();
        let mut values = Vec::new();
        for &(field, value) in filters {
            if !FILTERS.contains(&field) {
                return Err(Error::Internal(format!("can't filter on '{field}'")));
            }
            if !value.is_empty() && columns.iter().any(|c| c.name == field) {
                // Stored as text, or as the same bytes stored as a blob (which reads as text too).
                conditions.push(format!("{field} IN (?, CAST(? AS BLOB))"));
                values.extend([value.to_string(), value.to_string()]);
            }
        }
        let filter = if conditions.is_empty() { String::new() } else { format!(" WHERE {}", conditions.join(" AND ")) };
        let sql = format!("SELECT * FROM {}{filter} ORDER BY {}", kind.table(), kind.order_by());
        let mut statement = self.conn().prepare_cached(&sql)?;
        let mut rows = statement.query(params_from_iter(values))?;
        // One unreadable record (e.g. edited by hand) shouldn't hide all the others.
        let mut docs = Vec::new();
        let mut unreadable = Vec::new();
        while let Some(row) = rows.next()? {
            match to_doc(kind, columns, row) {
                Ok(doc) => docs.push(doc),
                Err(e) => {
                    // Null reads as empty; text that isn't text (e.g. a blob) as `Err`.
                    let text = |name: &str| row.get::<_, Option<String>>(name).map(Option::unwrap_or_default);
                    let has_campaign = columns.iter().any(|c| c.name == "campaign_id");
                    unreadable.push(Unreadable {
                        kind,
                        id: text("id").unwrap_or_default(),
                        campaign_id: if has_campaign { text("campaign_id").ok() } else { Some(String::new()) },
                        error: e.to_string(),
                    });
                }
            }
        }
        Ok((docs, unreadable))
    }

    /// Every record that can't be read, of every kind.
    pub async fn unreadable(&mut self) -> Result<Vec<Unreadable>> {
        let mut out = Vec::new();
        for kind in Kind::ALL {
            out.extend(self.list_checked(kind, &[]).await?.1);
        }
        Ok(out)
    }

    pub async fn get(&mut self, kind: Kind, id: &str) -> Result<Value> {
        self.find(kind, id).await?.ok_or_else(|| not_found(format!("{} {id} not found", kind.singular())))
    }

    pub(crate) async fn find(&mut self, kind: Kind, id: &str) -> Result<Option<Value>> {
        if !valid_id(id) {
            return Ok(None);
        }
        let columns = self.columns(kind);
        let sql = format!("SELECT * FROM {} WHERE id = ?", kind.table());
        let mut statement = self.conn().prepare_cached(&sql)?;
        let mut rows = statement.query([id])?;
        rows.next()?.map(|row| to_doc(kind, columns, row)).transpose()
    }

    pub async fn exists(&mut self, kind: Kind, id: &str) -> Result<bool> {
        if !valid_id(id) {
            return Ok(false);
        }
        let sql = format!("SELECT 1 FROM {} WHERE id = ?", kind.table());
        let found = self.conn().prepare_cached(&sql)?.query_row([id], |_| Ok(())).optional()?;
        Ok(found.is_some())
    }

    /// A record as its schema type, for game logic.
    pub async fn get_as<T: DeserializeOwned>(&mut self, kind: Kind, id: &str) -> Result<T> {
        let doc = self.get(kind, id).await?;
        Ok(serde_json::from_value(doc).expect("normalized records fit their schema"))
    }

    // ------------------------------------------------------------------ writes

    /// Insert or replace a whole record. Timestamps are set here unless
    /// `keep_timestamps` is given and the record has them (imports).
    pub async fn save(&mut self, kind: Kind, data: Value, keep_timestamps: bool) -> Result<Value> {
        let mut doc = kind.normalize(data).map_err(Error::Invalid)?;
        let text = |doc: &Value, key: &str| doc[key].as_str().unwrap_or("").to_string();
        let mut id = text(&doc, "id");
        if id.is_empty() {
            id = new_id();
        }
        if !valid_id(&id) {
            return Err(invalid(format!("invalid id '{id}'")));
        }
        for (field, target) in LINKS {
            let link = doc.get(field).and_then(Value::as_str).unwrap_or("").to_string();
            if !link.is_empty() && !self.exists(target, &link).await? {
                return Err(invalid(format!("{} {link} not found", target.singular())));
            }
        }
        let stored_created = match self.find(kind, &id).await? {
            Some(existing) => text(&existing, "created"),
            None => String::new(),
        };
        let stamp = now();
        let created =
            [text(&doc, "created"), stored_created].into_iter().find(|c| !c.is_empty()).unwrap_or(stamp.clone());
        let updated = if keep_timestamps && !text(&doc, "updated").is_empty() { text(&doc, "updated") } else { stamp };
        doc["id"] = Value::String(id.clone());
        doc["created"] = Value::String(created);
        doc["updated"] = Value::String(updated);

        let columns = self.columns(kind);
        let names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();
        let updates: Vec<String> =
            names.iter().filter(|&&n| n != "id").map(|n| format!("{n} = excluded.{n}")).collect();
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({}) ON CONFLICT(id) DO UPDATE SET {}",
            kind.table(),
            names.join(", "),
            vec!["?"; names.len()].join(", "),
            updates.join(", ")
        );
        let values = columns.iter().map(|column| {
            let value = doc.get(&column.name).cloned().unwrap_or(Value::Null);
            match (column.kind, value) {
                (ColumnKind::Link, Value::String(s)) if s.is_empty() => ToSqlOutput::from(rusqlite::types::Null),
                (ColumnKind::Json, value) => ToSqlOutput::from(value.to_string()),
                (_, Value::Bool(b)) => ToSqlOutput::from(b),
                (_, Value::Number(n)) if n.is_i64() => ToSqlOutput::from(n.as_i64().unwrap_or_default()),
                (_, Value::Number(n)) => ToSqlOutput::from(n.as_f64().unwrap_or_default()),
                (_, Value::String(s)) => ToSqlOutput::from(s),
                (_, Value::Null) => ToSqlOutput::from(rusqlite::types::Null),
                (_, other) => ToSqlOutput::from(other.to_string()),
            }
        });
        self.conn().prepare_cached(&sql)?.execute(params_from_iter(values))?;
        self.get(kind, &id).await
    }

    /// A schema value saved as a whole record.
    pub async fn save_as<T: Serialize>(&mut self, kind: Kind, doc: &T) -> Result<Value> {
        self.save(kind, serde_json::to_value(doc).expect("schemas serialize"), false).await
    }

    /// A new record from partial data (missing fields get defaults; any id or created time is replaced).
    pub async fn create(&mut self, kind: Kind, data: Option<Value>) -> Result<Value> {
        let mut data = match data {
            None | Some(Value::Null) => Map::new(),
            Some(Value::Object(map)) => map,
            Some(_) => return Err(invalid(format!("{} must be an object", kind.singular()))),
        };
        data.insert("id".into(), Value::String(new_id()));
        data.remove("created");
        self.save(kind, Value::Object(data), false).await
    }

    /// Merge `data` over the existing record's top-level fields.
    pub async fn update(&mut self, kind: Kind, id: &str, data: Value) -> Result<Value> {
        let Value::Object(changes) = data else {
            return Err(invalid(format!("{} must be an object", kind.singular())));
        };
        let mut merged = self.get(kind, id).await?;
        let created = merged["created"].clone();
        let fields = merged.as_object_mut().expect("records are objects");
        fields.extend(changes);
        fields.insert("id".into(), Value::String(id.to_string()));
        fields.insert("created".into(), created);
        self.save(kind, merged, false).await
    }

    /// Delete a record. Linked records follow the foreign keys: a campaign's sessions and
    /// DM screen records go with it; characters and handbook entries are unlinked.
    pub async fn delete(&mut self, kind: Kind, id: &str) -> Result<()> {
        if !self.exists(kind, id).await? {
            return Err(not_found(format!("{} {id} not found", kind.singular())));
        }
        let sql = format!("DELETE FROM {} WHERE id = ?", kind.table());
        self.conn().prepare_cached(&sql)?.execute([id])?;
        Ok(())
    }
}

impl Repo<'_> {
    // ------------------------------------------------------------------ rules data edits

    /// Every edit to the character builder's rules data.
    pub async fn rule_overrides(&mut self) -> Result<Vec<Override>> {
        let mut statement = self
            .conn()
            .prepare_cached(r#"SELECT kind, "key", data, updated FROM rule_overrides ORDER BY kind, "key""#)?;
        let rows = statement.query_map([], |row| {
            let data: Option<String> = row.get(2)?;
            Ok(Override {
                kind: row.get(0)?,
                key: row.get(1)?,
                data: data.map(parse_stored_json).filter(|v| !v.is_null()),
                updated: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Store an edit (`data: None` hides a bundled entry), replacing any earlier one. `updated`
    /// defaults to now (imports keep the time the edit was made).
    pub async fn save_rule_override(
        &mut self,
        kind: &str,
        key: &str,
        data: Option<&Value>,
        updated: Option<&str>,
    ) -> Result<()> {
        self.conn().execute(
            r#"INSERT INTO rule_overrides (kind, "key", data, updated) VALUES (?, ?, ?, ?)
               ON CONFLICT (kind, "key") DO UPDATE SET data = excluded.data, updated = excluded.updated"#,
            params![kind, key, data.map(Value::to_string), updated.map_or_else(now, str::to_string)],
        )?;
        Ok(())
    }

    /// Drop an edit, bringing back the bundled entry. False if there was none.
    pub async fn delete_rule_override(&mut self, kind: &str, key: &str) -> Result<bool> {
        let done = self.conn().execute(r#"DELETE FROM rule_overrides WHERE kind = ? AND "key" = ?"#, [kind, key])?;
        Ok(done > 0)
    }
}

impl Drop for Repo<'_> {
    /// Roll back anything not committed, and give the connection back to the pool.
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            if !conn.is_autocommit() {
                let _ = conn.execute_batch("ROLLBACK");
            }
            self.pool.put(conn);
        }
    }
}

/// A row as a normalized record.
fn to_doc(kind: Kind, columns: &[Column], row: &Row) -> Result<Value> {
    let mut doc = Map::new();
    for column in columns {
        doc.insert(column.name.clone(), read_cell(row, column)?);
    }
    kind.normalize(Value::Object(doc))
        .map_err(|e| Error::Internal(format!("stored {} is invalid: {e}", kind.singular())))
}

/// One cell, by what it actually holds (SQLite columns can hold any type) and what the
/// column is for.
fn read_cell(row: &Row, column: &Column) -> Result<Value> {
    let value = match row.get_ref(column.name.as_str())? {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(n) if column.kind == ColumnKind::Boolean => Value::Bool(n != 0),
        ValueRef::Integer(n) => Value::from(n),
        ValueRef::Real(f) => Value::from(f),
        ValueRef::Text(bytes) | ValueRef::Blob(bytes) => {
            let text = String::from_utf8(bytes.to_vec())
                .map_err(|_| Error::Internal(format!("{} isn't text", column.name)))?;
            match column.kind {
                ColumnKind::Json => parse_stored_json(text),
                _ => Value::String(text),
            }
        }
    };
    Ok(value)
}

/// Stored JSON. Python writes `NaN` and `Infinity` for such floats, which aren't JSON;
/// they're read as 0. Anything else unparseable stays text (and fails validation).
fn parse_stored_json(text: String) -> Value {
    if let Ok(value) = serde_json::from_str(&text) {
        return value;
    }
    let mut cleaned = String::with_capacity(text.len());
    let mut in_string = false;
    let mut escaped = false;
    let mut rest = text.as_str();
    while let Some(ch) = rest.chars().next() {
        if !in_string {
            let token = ["-Infinity", "Infinity", "NaN"].into_iter().find(|t| rest.starts_with(t));
            if let Some(token) = token {
                cleaned.push('0');
                rest = &rest[token.len()..];
                continue;
            }
        }
        match ch {
            '"' if !escaped => in_string = !in_string,
            _ => {}
        }
        escaped = in_string && ch == '\\' && !escaped;
        cleaned.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    serde_json::from_str(&cleaned).unwrap_or(Value::String(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_nan_reads_as_zero() {
        let value = parse_stored_json(r#"[{"name": "NaN bread", "weight": NaN}, -Infinity]"#.to_string());
        assert_eq!(value, serde_json::json!([{"name": "NaN bread", "weight": 0}, 0]));
        assert_eq!(parse_stored_json("{oops".into()), Value::String("{oops".into()));
    }
}
