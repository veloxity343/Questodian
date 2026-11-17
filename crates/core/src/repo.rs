//! Reading and writing records, the same way for every kind.
//!
//! Records travel as JSON in the shape of [`crate::schemas`]. Rows are converted using
//! the column types the database declares (see [`ColumnKind`]), so there is no
//! per-table mapping code. A [`Repo`] is one transaction: nothing is saved until
//! [`Repo::commit`], and dropping it rolls everything back.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, Sqlite, Transaction, TypeInfo, ValueRef};

use crate::builder::Override;
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

pub struct Repo<'a> {
    tx: Transaction<'static, Sqlite>,
    columns: &'a HashMap<Kind, Vec<Column>>,
    /// Where handout images live.
    pub files_dir: PathBuf,
}

impl<'a> Repo<'a> {
    pub(crate) fn new(
        tx: Transaction<'static, Sqlite>,
        columns: &'a HashMap<Kind, Vec<Column>>,
        files_dir: PathBuf,
    ) -> Self {
        Repo { tx, columns, files_dir }
    }

    /// Save everything done in this unit of work.
    pub async fn commit(self) -> Result<()> {
        self.tx.commit().await?;
        Ok(())
    }

    fn columns(&self, kind: Kind) -> &'a [Column] {
        &self.columns[&kind]
    }

    // ------------------------------------------------------------------ reads

    /// All records of a kind, optionally filtered (empty values are ignored).
    pub async fn list(&mut self, kind: Kind, filters: &[(&str, &str)]) -> Result<Vec<Value>> {
        let columns = self.columns(kind);
        let mut conditions = Vec::new();
        let mut values = Vec::new();
        for &(field, value) in filters {
            if !FILTERS.contains(&field) {
                return Err(Error::Internal(format!("can't filter on '{field}'")));
            }
            if !value.is_empty() && columns.iter().any(|c| c.name == field) {
                conditions.push(format!("{field} = ?"));
                values.push(value.to_string());
            }
        }
        let filter = if conditions.is_empty() { String::new() } else { format!(" WHERE {}", conditions.join(" AND ")) };
        let sql = format!("SELECT * FROM {}{filter} ORDER BY {}", kind.table(), kind.order_by());
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
        for value in values {
            query = query.bind(value);
        }
        let rows = query.fetch_all(&mut *self.tx).await?;
        // One unreadable record (e.g. edited by hand) shouldn't hide all the others.
        let mut docs = Vec::with_capacity(rows.len());
        for row in &rows {
            match to_doc(kind, columns, row) {
                Ok(doc) => docs.push(doc),
                Err(e) => eprintln!("questodian: skipping a {} that can't be read: {e}", kind.singular()),
            }
        }
        Ok(docs)
    }

    pub async fn get(&mut self, kind: Kind, id: &str) -> Result<Value> {
        self.find(kind, id).await?.ok_or_else(|| not_found(format!("{} {id} not found", kind.singular())))
    }

    async fn find(&mut self, kind: Kind, id: &str) -> Result<Option<Value>> {
        if !valid_id(id) {
            return Ok(None);
        }
        let sql = format!("SELECT * FROM {} WHERE id = ?", kind.table());
        let row = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(id).fetch_optional(&mut *self.tx).await?;
        row.map(|row| to_doc(kind, self.columns(kind), &row)).transpose()
    }

    pub async fn exists(&mut self, kind: Kind, id: &str) -> Result<bool> {
        if !valid_id(id) {
            return Ok(false);
        }
        let sql = format!("SELECT 1 FROM {} WHERE id = ?", kind.table());
        Ok(sqlx::query(sqlx::AssertSqlSafe(sql)).bind(id).fetch_optional(&mut *self.tx).await?.is_some())
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
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
        for column in columns {
            let value = doc.get(&column.name).cloned().unwrap_or(Value::Null);
            query = match (column.kind, value) {
                (ColumnKind::Link, Value::String(s)) if s.is_empty() => query.bind(None::<String>),
                (ColumnKind::Json, value) => query.bind(value.to_string()),
                (_, Value::Bool(b)) => query.bind(b),
                (_, Value::Number(n)) if n.is_i64() => query.bind(n.as_i64()),
                (_, Value::Number(n)) => query.bind(n.as_f64()),
                (_, Value::String(s)) => query.bind(s),
                (_, Value::Null) => query.bind(None::<String>),
                (_, other) => query.bind(other.to_string()),
            };
        }
        query.execute(&mut *self.tx).await?;
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
        sqlx::query(sqlx::AssertSqlSafe(sql)).bind(id).execute(&mut *self.tx).await?;
        Ok(())
    }
}

impl Repo<'_> {
    // ------------------------------------------------------------------ rules data edits

    /// Every edit to the character builder's rules data.
    pub async fn rule_overrides(&mut self) -> Result<Vec<Override>> {
        let rows: Vec<(String, String, Option<String>, String)> =
            // The column is declared JSON, so SQLite stores a bare number as one; read it back as text.
            sqlx::query_as(r#"SELECT kind, "key", CAST(data AS TEXT), updated FROM rule_overrides ORDER BY kind, "key""#)
                .fetch_all(&mut *self.tx)
                .await?;
        Ok(rows
            .into_iter()
            .map(|(kind, key, data, updated)| Override {
                kind,
                key,
                data: data.map(parse_stored_json).filter(|v| !v.is_null()),
                updated,
            })
            .collect())
    }

    /// Store an edit (`data: None` hides a bundled entry), replacing any earlier one.
    pub async fn save_rule_override(&mut self, kind: &str, key: &str, data: Option<&Value>) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO rule_overrides (kind, "key", data, updated) VALUES (?, ?, ?, ?)
               ON CONFLICT (kind, "key") DO UPDATE SET data = excluded.data, updated = excluded.updated"#,
        )
        .bind(kind)
        .bind(key)
        .bind(data.map(Value::to_string))
        .bind(now())
        .execute(&mut *self.tx)
        .await?;
        Ok(())
    }

    /// Drop an edit, bringing back the bundled entry. False if there was none.
    pub async fn delete_rule_override(&mut self, kind: &str, key: &str) -> Result<bool> {
        let done = sqlx::query(r#"DELETE FROM rule_overrides WHERE kind = ? AND "key" = ?"#)
            .bind(kind)
            .bind(key)
            .execute(&mut *self.tx)
            .await?;
        Ok(done.rows_affected() > 0)
    }
}

/// A row as a normalized record.
fn to_doc(kind: Kind, columns: &[Column], row: &SqliteRow) -> Result<Value> {
    let mut doc = Map::new();
    for column in columns {
        doc.insert(column.name.clone(), read_cell(row, column)?);
    }
    kind.normalize(Value::Object(doc))
        .map_err(|e| Error::Internal(format!("stored {} is invalid: {e}", kind.singular())))
}

/// One cell, by what it actually holds (SQLite columns can hold any type) and what the
/// column is for.
fn read_cell(row: &SqliteRow, column: &Column) -> Result<Value> {
    let name = column.name.as_str();
    let raw = row.try_get_raw(name)?;
    if raw.is_null() {
        return Ok(Value::Null);
    }
    let storage = raw.type_info().name().to_string();
    let value = match storage.as_str() {
        "INTEGER" => {
            let n: i64 = row.try_get(name)?;
            if column.kind == ColumnKind::Boolean { Value::Bool(n != 0) } else { Value::from(n) }
        }
        "REAL" => Value::from(row.try_get::<f64, _>(name)?),
        _ => {
            let text: String = row.try_get(name)?;
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
