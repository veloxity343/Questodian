//! Dice notation and a safe arithmetic evaluator.
//!
//! One engine powers both the dice roller and the calculator: an expression is
//! ordinary arithmetic in which dice terms may appear anywhere.
//!
//! Dice syntax (case-insensitive):
//!
//! ```text
//! d20        one twenty-sided die
//! 4d6kh3     roll 4d6, keep the highest 3   (also k3)
//! 2d20kl1    keep lowest (disadvantage)
//! 4d6dl1     drop lowest                    (also dh)
//! 2d20kh     a missing count means 1
//! 3d6!       exploding: roll again on max (capped)
//! 2d6ro1     reroll 1s once (ro2 = Great Weapon Fighting)
//! 2d6r1      reroll 1s until they stop being 1s (capped)
//! d%         percentile die (d100)
//! 4dF        Fate/Fudge dice (-1, 0, +1)
//! ```
//!
//! Arithmetic: `+ - * / // % ^` (`**` is a synonym for `^`) and parentheses, plus
//! `floor ceil round abs min max sqrt`, the constants `pi` and `e`, and the D&D
//! helpers `mod(score)` (ability modifier) and `prof(level)` (proficiency bonus).
//! The shorthands `adv`, `dis` and `stats` expand to `2d20kh1`, `2d20kl1` and `4d6dl1`.
//!
//! The parser reads characters directly (no regular expressions) into a small
//! syntax tree, which is then evaluated with a dice roller.

use rand::RngExt;
use serde::Serialize;
use serde_json::Value;

pub const MAX_DICE: u32 = 1000;
pub const MAX_SIDES: u32 = 100_000;
pub const MAX_EXPLOSIONS: u32 = 100;
pub const MAX_EXPR_LENGTH: usize = 500;
/// Dice rolled in one expression, rerolls and explosions included.
const MAX_ROLLED: u32 = MAX_DICE * 10;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{0}")]
pub struct DiceError(pub String);

fn fail<T>(message: impl Into<String>) -> Result<T, DiceError> {
    Err(DiceError(message.into()))
}

// --------------------------------------------------------------------------- randomness

/// Where die faces come from. Any `rand` generator works; tests use fixed sequences.
pub trait DieSource {
    /// A whole number from `low` to `high`, inclusive.
    fn between(&mut self, low: i64, high: i64) -> i64;
}

impl<R: RngExt + ?Sized> DieSource for R {
    fn between(&mut self, low: i64, high: i64) -> i64 {
        self.random_range(low..=high)
    }
}

// --------------------------------------------------------------------------- results

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DieRoll {
    pub value: i64,
    pub kept: bool,
    /// Rolled because the previous die exploded.
    #[serde(skip_serializing_if = "is_false")]
    pub exploded: bool,
    /// Replaces a die that was rerolled.
    #[serde(skip_serializing_if = "is_false")]
    pub rerolled: bool,
    /// Rerolled away; doesn't count.
    #[serde(skip_serializing_if = "is_false")]
    pub discarded: bool,
}

fn is_false(b: &bool) -> bool {
    !b
}

impl DieRoll {
    fn new(value: i64) -> Self {
        DieRoll { value, kept: true, exploded: false, rerolled: false, discarded: false }
    }

    fn counts(&self) -> bool {
        self.kept && !self.discarded
    }
}

/// A die's faces: a number of sides, or Fate dice (-1, 0, +1), shown as `"F"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sides {
    Number(u32),
    Fate,
}

impl Serialize for Sides {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Sides::Number(n) => s.serialize_u32(*n),
            Sides::Fate => s.serialize_str("F"),
        }
    }
}

impl Sides {
    fn highest(self) -> i64 {
        match self {
            Sides::Number(n) => n as i64,
            Sides::Fate => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiceGroup {
    /// The dice term as written, e.g. `4d6dl1`.
    pub notation: String,
    pub count: u32,
    pub sides: Sides,
    pub total: i64,
    pub rolls: Vec<DieRoll>,
}

impl DiceGroup {
    /// `4d6dl1 [~~1~~, 3, 5, 6]`: dropped dice struck through, exploded ones marked `!`.
    fn describe(&self) -> String {
        let faces: Vec<String> = self
            .rolls
            .iter()
            .map(|r| {
                let mut text = r.value.to_string();
                if !r.counts() {
                    text = format!("~~{text}~~");
                }
                if r.exploded {
                    text.push('!');
                }
                text
            })
            .collect();
        format!("{} [{}]", self.notation, faces.join(", "))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RollResult {
    /// The expression after shorthand expansion (`adv+5` becomes `2d20kh1+5`).
    pub expression: String,
    /// A whole number when the result is whole, otherwise a decimal.
    pub total: Value,
    /// The expression with every die shown, e.g. `2d20kh1 [~~4~~, 17] + 5 = 22`.
    pub breakdown: String,
    pub groups: Vec<DiceGroup>,
    /// The kept d20 when the roll hinges on exactly one d20, for crits and fumbles.
    pub natural: Option<i64>,
    pub critical: bool,
    pub fumble: bool,
}

impl RollResult {
    pub fn total_f64(&self) -> f64 {
        self.total.as_f64().unwrap_or(0.0)
    }
}

// --------------------------------------------------------------------------- public API

/// Evaluate a dice or arithmetic expression with a fresh random generator.
pub fn roll(expression: &str) -> Result<RollResult, DiceError> {
    roll_with(expression, &mut rand::rng())
}

/// Evaluate a dice or arithmetic expression using the given source of dice.
pub fn roll_with(expression: &str, dice: &mut impl DieSource) -> Result<RollResult, DiceError> {
    let expression = expand_shorthand(expression.trim());
    let tree = parse(&expression)?;
    let mut roller = Roller { dice, groups: Vec::new(), rolled: 0 };
    let (value, text) = roller.eval(&tree)?;
    if !value.is_finite() {
        return fail("number too large");
    }
    let shown = format_number(value);
    let breakdown = if text == shown { shown } else { format!("{text} = {shown}") };
    let natural = natural_d20(&roller.groups);
    Ok(RollResult {
        expression,
        total: clean(value),
        breakdown,
        groups: roller.groups,
        natural,
        critical: natural == Some(20),
        fumble: natural == Some(1),
    })
}

/// Evaluate an expression and return just the number.
pub fn calculate(expression: &str) -> Result<f64, DiceError> {
    Ok(roll(expression)?.total_f64())
}

/// A standard ability-score array: six sets of 4d6, dropping the lowest die.
pub fn roll_stats_with(dice: &mut impl DieSource) -> Vec<RollResult> {
    (0..6).map(|_| roll_with("4d6dl1", dice).expect("4d6dl1 is valid")).collect()
}

pub fn roll_stats() -> Vec<RollResult> {
    roll_stats_with(&mut rand::rng())
}

/// Replace whole words `adv`, `advantage`, `dis`, `disadvantage` and `stats`.
fn expand_shorthand(expression: &str) -> String {
    let mut out = String::with_capacity(expression.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        let replacement = match word.to_lowercase().as_str() {
            "adv" | "advantage" => Some("2d20kh1"),
            "dis" | "disadvantage" => Some("2d20kl1"),
            "stats" => Some("4d6dl1"),
            _ => None,
        };
        out.push_str(replacement.unwrap_or(word));
        word.clear();
    };
    for ch in expression.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            word.push(ch);
        } else {
            flush(&mut word, &mut out);
            out.push(ch);
        }
    }
    flush(&mut word, &mut out);
    out
}

fn natural_d20(groups: &[DiceGroup]) -> Option<i64> {
    let mut d20s = groups.iter().filter(|g| g.sides == Sides::Number(20));
    let (Some(only), None) = (d20s.next(), d20s.next()) else { return None };
    let mut kept = only.rolls.iter().filter(|r| r.counts());
    match (kept.next(), kept.next()) {
        (Some(die), None) => Some(die.value),
        _ => None,
    }
}

// --------------------------------------------------------------------------- number formatting

/// Whole numbers print without a decimal point; anything else like C's `%.6g`.
pub fn format_number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 { format!("{}", v as i64) } else { format_g6(v) }
}

/// The total as JSON: an integer when whole, else rounded to 10 decimal places.
fn clean(v: f64) -> Value {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        Value::from(v as i64)
    } else if v.abs() < 1e15 {
        Value::from((v * 1e10).round() / 1e10)
    } else {
        Value::from(v)
    }
}

/// `%.6g`: six significant digits, trailing zeros removed, exponent form when the
/// exponent is below -4 or at least 6.
fn format_g6(v: f64) -> String {
    if !v.is_finite() {
        return if v.is_nan() {
            "nan".into()
        } else if v > 0.0 {
            "inf".into()
        } else {
            "-inf".into()
        };
    }
    let scientific = format!("{v:.5e}"); // e.g. "1.23457e6"
    let (mantissa, exponent) = scientific.split_once('e').expect("formatted with an exponent");
    let exponent: i32 = exponent.parse().expect("exponent is a number");
    if !(-4..6).contains(&exponent) {
        let sign = if exponent < 0 { '-' } else { '+' };
        format!("{}e{sign}{:02}", trim_zeros(mantissa), exponent.abs())
    } else {
        let decimals = (5 - exponent) as usize;
        trim_zeros(&format!("{v:.decimals$}")).to_string()
    }
}

fn trim_zeros(number: &str) -> &str {
    if number.contains('.') { number.trim_end_matches('0').trim_end_matches('.') } else { number }
}

// --------------------------------------------------------------------------- syntax tree

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Number(f64),
    Dice(DiceTerm),
    Negate(Box<Expr>),
    /// Unary plus: kept so the tree mirrors the input, but it changes nothing.
    Plus(Box<Expr>),
    Binary(Op, Box<Expr>, Box<Expr>),
    Group(Box<Expr>),
    Call(Function, Vec<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Add,
    Subtract,
    Multiply,
    Divide,
    FloorDivide,
    Remainder,
    Power,
}

impl Op {
    fn symbol(self) -> &'static str {
        match self {
            Op::Add => "+",
            Op::Subtract => "-",
            Op::Multiply => "*",
            Op::Divide => "/",
            Op::FloorDivide => "//",
            Op::Remainder => "%",
            Op::Power => "^",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Function {
    Floor,
    Ceil,
    Round,
    Abs,
    Min,
    Max,
    Sqrt,
    Mod,
    Prof,
}

impl Function {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "floor" => Function::Floor,
            "ceil" => Function::Ceil,
            "round" => Function::Round,
            "abs" => Function::Abs,
            "min" => Function::Min,
            "max" => Function::Max,
            "sqrt" => Function::Sqrt,
            "mod" => Function::Mod,
            "prof" => Function::Prof,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Function::Floor => "floor",
            Function::Ceil => "ceil",
            Function::Round => "round",
            Function::Abs => "abs",
            Function::Min => "min",
            Function::Max => "max",
            Function::Sqrt => "sqrt",
            Function::Mod => "mod",
            Function::Prof => "prof",
        }
    }

    /// How many arguments it takes, as (fewest, most).
    fn arity(self) -> (usize, usize) {
        match self {
            Function::Round => (1, 2),
            Function::Min | Function::Max => (1, 99),
            _ => (1, 1),
        }
    }

    fn apply(self, args: &[f64]) -> Result<f64, DiceError> {
        let x = args[0];
        Ok(match self {
            Function::Floor => x.floor(),
            Function::Ceil => x.ceil(),
            Function::Round => {
                // round(x) rounds halves up; round(x, n) rounds to n places, halves to even.
                let places = args.get(1).map_or(0.0, |n| n.trunc());
                if places == 0.0 {
                    (x + 0.5).floor()
                } else {
                    let scale = 10f64.powf(places);
                    (x * scale).round_ties_even() / scale
                }
            }
            Function::Abs => x.abs(),
            Function::Min => args.iter().copied().fold(f64::INFINITY, f64::min),
            Function::Max => args.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            Function::Sqrt if x < 0.0 => return fail("sqrt of a negative number"),
            Function::Sqrt => x.sqrt(),
            Function::Mod => ((x - 10.0) / 2.0).floor(),
            Function::Prof => {
                let level = x.trunc();
                if !(1.0..=30.0).contains(&level) {
                    return fail("prof() takes a level from 1 to 30");
                }
                2.0 + ((level - 1.0) / 4.0).floor()
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
struct DiceTerm {
    notation: String,
    count: u32,
    sides: Sides,
    keep: Option<Keep>,
    /// Reroll faces at or below this, once.
    reroll_once: Option<i64>,
    /// Reroll faces at or below this, until they're higher (capped).
    reroll: Option<i64>,
    explode: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Keep {
    Highest(u32),
    Lowest(u32),
    DropHighest(u32),
    DropLowest(u32),
}

// --------------------------------------------------------------------------- parser

/// Recursive descent over the characters of the expression.
///
/// ```text
/// expr  = term (("+" | "-") term)*
/// term  = unary (("*" | "/" | "//" | "%") unary)*
/// unary = ("-" | "+") unary | power
/// power = atom ("^" unary)?            right-associative: 2^3^2 = 2^9
/// atom  = number | dice | name | name "(" expr ("," expr)* ")" | "(" expr ")"
/// ```
fn parse(expression: &str) -> Result<Expr, DiceError> {
    let chars: Vec<char> = expression.trim().chars().collect();
    if chars.len() > MAX_EXPR_LENGTH {
        return fail(format!("expression too long (max {MAX_EXPR_LENGTH} characters)"));
    }
    // Characters that can't appear anywhere are reported first, wherever they are.
    let allowed = |c: &char| c.is_whitespace() || c.is_ascii_alphanumeric() || "_.+-*/%^(),!".contains(*c);
    if let Some(i) = chars.iter().position(|c| !allowed(c)) {
        return fail(format!("unexpected character '{}' at position {}", chars[i], i + 1));
    }
    let mut parser = Parser { chars, pos: 0 };
    if parser.at_end() {
        return fail("empty expression");
    }
    let tree = parser.expr()?;
    if !parser.at_end() {
        let start = parser.pos;
        let rest: String = parser.chars[start..].iter().take(8).collect();
        return fail(format!("unexpected '{rest}' at position {}", start + 1));
    }
    Ok(tree)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn skip_space(&mut self) {
        while self.chars.get(self.pos).is_some_and(|c| c.is_whitespace()) {
            self.pos += 1;
        }
    }

    fn at_end(&mut self) -> bool {
        self.skip_space();
        self.pos >= self.chars.len()
    }

    fn peek_char(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    /// The operator at the cursor, without consuming it.
    fn peek_op(&mut self) -> Option<(Op, usize)> {
        self.skip_space();
        let pair = (self.peek_char(0)?, self.peek_char(1));
        Some(match pair {
            ('/', Some('/')) => (Op::FloorDivide, 2),
            ('*', Some('*')) => (Op::Power, 2),
            ('+', _) => (Op::Add, 1),
            ('-', _) => (Op::Subtract, 1),
            ('*', _) => (Op::Multiply, 1),
            ('/', _) => (Op::Divide, 1),
            ('%', _) => (Op::Remainder, 1),
            ('^', _) => (Op::Power, 1),
            _ => return None,
        })
    }

    fn accept_op(&mut self, allowed: &[Op]) -> Option<Op> {
        let (op, width) = self.peek_op()?;
        if !allowed.contains(&op) {
            return None;
        }
        self.pos += width;
        Some(op)
    }

    fn accept_char(&mut self, want: char) -> bool {
        self.skip_space();
        if self.peek_char(0) == Some(want) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect_char(&mut self, want: char) -> Result<(), DiceError> {
        if self.accept_char(want) {
            return Ok(());
        }
        let place = if self.at_end() { "end of input".to_string() } else { format!("position {}", self.pos + 1) };
        fail(format!("expected '{want}' at {place}"))
    }

    fn expr(&mut self) -> Result<Expr, DiceError> {
        let mut left = self.term()?;
        while let Some(op) = self.accept_op(&[Op::Add, Op::Subtract]) {
            left = Expr::Binary(op, Box::new(left), Box::new(self.term()?));
        }
        Ok(left)
    }

    fn term(&mut self) -> Result<Expr, DiceError> {
        let mut left = self.unary()?;
        while let Some(op) = self.accept_op(&[Op::Multiply, Op::Divide, Op::FloorDivide, Op::Remainder]) {
            left = Expr::Binary(op, Box::new(left), Box::new(self.unary()?));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, DiceError> {
        match self.accept_op(&[Op::Subtract, Op::Add]) {
            Some(Op::Subtract) => Ok(Expr::Negate(Box::new(self.unary()?))),
            Some(_) => Ok(Expr::Plus(Box::new(self.unary()?))),
            None => self.power(),
        }
    }

    fn power(&mut self) -> Result<Expr, DiceError> {
        let base = self.atom()?;
        if self.accept_op(&[Op::Power]).is_some() {
            return Ok(Expr::Binary(Op::Power, Box::new(base), Box::new(self.unary()?)));
        }
        Ok(base)
    }

    fn atom(&mut self) -> Result<Expr, DiceError> {
        if self.at_end() {
            return fail("unexpected end of expression");
        }
        if let Some(dice) = self.dice()? {
            return Ok(Expr::Dice(dice));
        }
        if let Some(number) = self.number() {
            return Ok(Expr::Number(number));
        }
        if let Some(name) = self.name() {
            return self.name_or_call(name);
        }
        let start = self.pos;
        let ch = self.chars[start];
        if ch == '(' {
            self.pos += 1;
            let inner = self.expr()?;
            self.expect_char(')')?;
            return Ok(Expr::Group(Box::new(inner)));
        }
        if self.peek_op().is_some() || ch == ')' || ch == ',' {
            let symbol = if self.peek_op().map(|(_, w)| w) == Some(2) {
                self.chars[start..start + 2].iter().collect::<String>().replace("**", "^")
            } else {
                ch.to_string()
            };
            return fail(format!("unexpected '{symbol}' at position {}", start + 1));
        }
        fail(format!("unexpected character '{ch}' at position {}", start + 1))
    }

    fn name_or_call(&mut self, name: String) -> Result<Expr, DiceError> {
        match name.as_str() {
            "pi" => return Ok(Expr::Number(std::f64::consts::PI)),
            "e" => return Ok(Expr::Number(std::f64::consts::E)),
            _ => {}
        }
        let Some(function) = Function::from_name(&name) else {
            return fail(format!("unknown name '{name}'"));
        };
        self.expect_char('(')?;
        let mut args = vec![self.expr()?];
        while self.accept_char(',') {
            args.push(self.expr()?);
        }
        self.expect_char(')')?;
        Ok(Expr::Call(function, args))
    }

    fn digits(&mut self) -> &[char] {
        let start = self.pos;
        while self.peek_char(0).is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        &self.chars[start..self.pos]
    }

    /// `12`, `1.5` or `.5`.
    fn number(&mut self) -> Option<f64> {
        let start = self.pos;
        self.digits();
        let whole_digits = self.pos > start;
        if self.peek_char(0) == Some('.') && self.peek_char(1).is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
            self.digits();
        } else if !whole_digits {
            return None;
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        Some(text.parse().expect("digits with at most one point"))
    }

    /// A letter or underscore, then letters, digits or underscores. Lowercased.
    fn name(&mut self) -> Option<String> {
        let first = self.peek_char(0)?;
        if !(first.is_ascii_alphabetic() || first == '_') {
            return None;
        }
        let start = self.pos;
        while self.peek_char(0).is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
            self.pos += 1;
        }
        Some(self.chars[start..self.pos].iter().collect::<String>().to_lowercase())
    }

    /// `[count] d (sides | % | F) modifiers*`, or nothing (cursor unchanged) if
    /// the text here isn't a dice term.
    fn dice(&mut self) -> Result<Option<DiceTerm>, DiceError> {
        let start = self.pos;
        let count_text: String = self.digits().iter().collect();
        if !matches!(self.peek_char(0), Some('d' | 'D')) {
            self.pos = start;
            return Ok(None);
        }
        self.pos += 1;
        let sides_text: String = self.digits().iter().collect();
        let sides = if !sides_text.is_empty() {
            Sides::Number(parse_capped(&sides_text))
        } else if self.peek_char(0) == Some('%') {
            self.pos += 1;
            Sides::Number(100)
        } else if matches!(self.peek_char(0), Some('f' | 'F')) {
            self.pos += 1;
            Sides::Fate
        } else {
            self.pos = start; // e.g. a name such as "dis"
            return Ok(None);
        };
        let modifiers = self.dice_modifiers();
        let notation: String = self.chars[start..self.pos].iter().collect();
        let count = if count_text.is_empty() { 1 } else { parse_capped(&count_text) };
        build_dice(notation, count, sides, &modifiers).map(Some)
    }

    /// Modifiers after a dice term, e.g. `kh3`, `ro1`, `!`. The number is `None` when omitted.
    fn dice_modifiers(&mut self) -> Vec<(String, Option<u32>)> {
        let mut modifiers = Vec::new();
        loop {
            let two: String = self.chars[self.pos..].iter().take(2).collect::<String>().to_lowercase();
            let kind = if ["kh", "kl", "dh", "dl", "ro"].contains(&two.as_str()) {
                two
            } else if two.starts_with('k') || two.starts_with('r') || two.starts_with('!') {
                two[..1].to_string()
            } else {
                break;
            };
            self.pos += kind.len();
            if kind == "!" {
                modifiers.push((kind, None));
                continue;
            }
            let digits: String = self.digits().iter().collect();
            let amount = (!digits.is_empty()).then(|| parse_capped(&digits));
            modifiers.push((kind, amount));
        }
        modifiers
    }
}

/// Parse digits, saturating instead of overflowing (any huge value is out of range anyway).
fn parse_capped(digits: &str) -> u32 {
    digits.parse().unwrap_or(u32::MAX)
}

fn build_dice(
    notation: String,
    count: u32,
    sides: Sides,
    modifiers: &[(String, Option<u32>)],
) -> Result<DiceTerm, DiceError> {
    if !(1..=MAX_DICE).contains(&count) {
        return fail(format!("dice count must be between 1 and {MAX_DICE}"));
    }
    if let Sides::Number(n) = sides
        && !(1..=MAX_SIDES).contains(&n)
    {
        return fail(format!("dice sides must be between 1 and {MAX_SIDES}"));
    }
    let mut term = DiceTerm { notation, count, sides, keep: None, reroll_once: None, reroll: None, explode: false };
    for (kind, amount) in modifiers {
        let n = amount.unwrap_or(1);
        let keep = match kind.as_str() {
            "k" | "kh" => Some(Keep::Highest(n)),
            "kl" => Some(Keep::Lowest(n)),
            "dh" => Some(Keep::DropHighest(n)),
            "dl" => Some(Keep::DropLowest(n)),
            _ => None,
        };
        if let Some(keep) = keep {
            if term.keep.is_some_and(|old| std::mem::discriminant(&old) != std::mem::discriminant(&keep)) {
                return fail("use only one keep/drop modifier per dice term");
            }
            match keep {
                Keep::Highest(n) | Keep::Lowest(n) if !(1..=count).contains(&n) => {
                    return fail(format!("can't keep {n} of {count} dice"));
                }
                Keep::DropHighest(n) | Keep::DropLowest(n) if n >= count => {
                    return fail(format!("can't drop {n} of {count} dice"));
                }
                _ => term.keep = Some(keep),
            }
            continue;
        }
        match (kind.as_str(), sides) {
            ("r" | "ro", Sides::Number(faces)) if n >= faces => {
                return fail(format!("reroll threshold {n} would reroll every face of a d{faces}"));
            }
            ("r", _) => term.reroll = Some(n as i64),
            ("ro", _) => term.reroll_once = Some(n as i64),
            ("!", Sides::Number(1) | Sides::Fate) => return fail("that die can't explode"),
            ("!", _) => term.explode = true,
            _ => unreachable!("dice_modifiers only returns known kinds"),
        }
    }
    Ok(term)
}

// --------------------------------------------------------------------------- evaluation

struct Roller<'a, D: DieSource> {
    dice: &'a mut D,
    groups: Vec<DiceGroup>,
    rolled: u32,
}

impl<D: DieSource> Roller<'_, D> {
    /// The value of a subtree and how it reads in the breakdown.
    fn eval(&mut self, expr: &Expr) -> Result<(f64, String), DiceError> {
        Ok(match expr {
            Expr::Number(n) => (*n, format_number(*n)),
            Expr::Dice(term) => {
                let group = self.roll_term(term)?;
                let shown = group.describe();
                let total = group.total as f64;
                self.groups.push(group);
                (total, shown)
            }
            Expr::Negate(inner) => {
                let (v, text) = self.eval(inner)?;
                (-v, format!("-{text}"))
            }
            Expr::Plus(inner) => self.eval(inner)?,
            Expr::Group(inner) => {
                let (v, text) = self.eval(inner)?;
                (v, format!("({text})"))
            }
            Expr::Binary(op, left, right) => {
                let (a, left_text) = self.eval(left)?;
                let (b, right_text) = self.eval(right)?;
                (arithmetic(*op, a, b)?, format!("{left_text} {} {right_text}", op.symbol()))
            }
            Expr::Call(function, args) => {
                let (fewest, most) = function.arity();
                if !(fewest..=most).contains(&args.len()) {
                    let range = if fewest == most { fewest.to_string() } else { format!("{fewest}-{most}") };
                    return fail(format!("{}() takes {range} argument(s)", function.name()));
                }
                let mut values = Vec::with_capacity(args.len());
                let mut texts = Vec::with_capacity(args.len());
                for arg in args {
                    let (v, text) = self.eval(arg)?;
                    values.push(v);
                    texts.push(text);
                }
                (function.apply(&values)?, format!("{}({})", function.name(), texts.join(", ")))
            }
        })
    }

    fn die(&mut self, sides: Sides) -> Result<DieRoll, DiceError> {
        self.rolled += 1;
        if self.rolled > MAX_ROLLED {
            return fail("too many dice rolled");
        }
        let value = match sides {
            Sides::Number(n) => self.dice.between(1, n as i64),
            Sides::Fate => self.dice.between(-1, 1),
        };
        Ok(DieRoll::new(value))
    }

    fn roll_term(&mut self, term: &DiceTerm) -> Result<DiceGroup, DiceError> {
        let mut rolls = Vec::new();
        for _ in 0..term.count {
            // Each die starts a chain: rerolls replace it, explosions add to it.
            let mut chain = vec![self.die(term.sides)?];
            for (threshold, limit) in [(term.reroll_once, 1), (term.reroll, MAX_EXPLOSIONS)] {
                let Some(threshold) = threshold else { continue };
                for _ in 0..limit {
                    let last = chain.last_mut().expect("chain starts with a die");
                    if last.value > threshold {
                        break;
                    }
                    last.discarded = true;
                    let mut replacement = self.die(term.sides)?;
                    replacement.rerolled = true;
                    chain.push(replacement);
                }
            }
            if term.explode {
                for _ in 0..MAX_EXPLOSIONS {
                    if chain.last().expect("chain starts with a die").value != term.sides.highest() {
                        break;
                    }
                    let mut extra = self.die(term.sides)?;
                    extra.exploded = true;
                    chain.push(extra);
                }
            }
            rolls.extend(chain);
        }
        if let Some(keep) = term.keep {
            apply_keep(&mut rolls, keep);
        }
        let total = rolls.iter().filter(|r| r.counts()).map(|r| r.value).sum();
        Ok(DiceGroup { notation: term.notation.clone(), count: term.count, sides: term.sides, total, rolls })
    }
}

/// Mark the dice that keep/drop leaves out. Ties go to the earlier die.
fn apply_keep(rolls: &mut [DieRoll], keep: Keep) {
    let mut live: Vec<usize> = (0..rolls.len()).filter(|&i| !rolls[i].discarded).collect();
    let highest_first = matches!(keep, Keep::Highest(_) | Keep::DropHighest(_));
    // A stable sort, so equal faces stay in the order they were rolled.
    if highest_first {
        live.sort_by(|&a, &b| rolls[b].value.cmp(&rolls[a].value));
    } else {
        live.sort_by_key(|&i| rolls[i].value);
    }
    let left_out = match keep {
        Keep::Highest(n) | Keep::Lowest(n) => &live[(n as usize).min(live.len())..],
        Keep::DropHighest(n) | Keep::DropLowest(n) => &live[..(n as usize).min(live.len())],
    };
    for &i in left_out {
        rolls[i].kept = false;
    }
}

fn arithmetic(op: Op, a: f64, b: f64) -> Result<f64, DiceError> {
    if matches!(op, Op::Divide | Op::FloorDivide | Op::Remainder) && b == 0.0 {
        return fail("division by zero");
    }
    let value = match op {
        Op::Add => a + b,
        Op::Subtract => a - b,
        Op::Multiply => a * b,
        Op::Divide => a / b,
        Op::FloorDivide => (a / b).floor(),
        // The remainder takes the divisor's sign, so -7 % 3 is 2.
        Op::Remainder => {
            let r = a % b;
            if r != 0.0 && (r < 0.0) != (b < 0.0) { r + b } else { r }
        }
        Op::Power => {
            if b.abs() > 1000.0 || (a.abs() > 1.0 && b * a.abs().log10() > 300.0) {
                return fail("exponent too large");
            }
            if a == 0.0 && b < 0.0 {
                return fail("division by zero");
            }
            if a < 0.0 && b.fract() != 0.0 {
                return fail("result is not a real number");
            }
            a.powf(b)
        }
    };
    if !value.is_finite() {
        return fail("number too large");
    }
    Ok(value)
}

// --------------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use std::collections::VecDeque;

    /// Dice that come up in a fixed order.
    struct Fixed(VecDeque<i64>);

    impl Fixed {
        fn new(values: &[i64]) -> Self {
            Fixed(values.iter().copied().collect())
        }
    }

    impl DieSource for Fixed {
        fn between(&mut self, _low: i64, _high: i64) -> i64 {
            self.0.pop_front().expect("ran out of fixed dice")
        }
    }

    /// Always the same face.
    struct Always(i64);

    impl DieSource for Always {
        fn between(&mut self, _low: i64, _high: i64) -> i64 {
            self.0
        }
    }

    fn calc(expr: &str) -> f64 {
        calculate(expr).unwrap_or_else(|e| panic!("{expr}: {e}"))
    }

    fn seeded(seed: u64) -> StdRng {
        StdRng::seed_from_u64(seed)
    }

    #[test]
    fn plain_arithmetic() {
        assert_eq!(calc("1 + 2 * 3"), 7.0);
        assert_eq!(calc("(1 + 2) * 3"), 9.0);
        assert_eq!(calc("2 ^ 3 ^ 2"), 512.0);
        assert_eq!(calc("2 ** 3"), 8.0);
        assert_eq!(calc("-3 + +5"), 2.0);
        assert_eq!(calc("7 // 2"), 3.0);
        assert_eq!(calc("-7 // 2"), -4.0);
        assert_eq!(calc("7 / 2"), 3.5);
        assert_eq!(calc("7 % 3"), 1.0);
        assert_eq!(calc("-7 % 3"), 2.0);
        assert_eq!(calc(".5 + 1.25"), 1.75);
    }

    #[test]
    fn functions() {
        assert_eq!(calc("mod(18)"), 4.0);
        assert_eq!(calc("mod(9)"), -1.0);
        assert_eq!(calc("prof(1) + prof(5) + prof(17)"), 2.0 + 3.0 + 6.0);
        assert_eq!(calc("max(1, 5, 3) + min(4, 2)"), 7.0);
        assert_eq!(calc("floor(7/2) + ceil(7/2)"), 7.0);
        assert_eq!(calc("round(2.5)"), 3.0);
        assert_eq!(calc("round(-2.5)"), -2.0);
        assert_eq!(calc("round(1.23456, 2)"), 1.23);
        assert_eq!(calc("abs(-4) + sqrt(16)"), 8.0);
        assert_eq!(calc("min(5)"), 5.0);
    }

    #[test]
    fn dice_ranges() {
        let cases = [
            ("d20", 1, 20),
            ("3d6", 3, 18),
            ("2d20kh1+5", 6, 25),
            ("4d6dl1", 3, 18),
            ("d%", 1, 100),
            ("4dF", -4, 4),
            ("8d6//2", 4, 24),
        ];
        let mut rng = seeded(1);
        for (expr, low, high) in cases {
            for _ in 0..300 {
                let total = roll_with(expr, &mut rng).unwrap().total_f64();
                assert!((low as f64..=high as f64).contains(&total), "{expr} gave {total}");
            }
        }
    }

    #[test]
    fn keep_highest_keeps_max() {
        let mut rng = seeded(7);
        for _ in 0..100 {
            let result = roll_with("2d20kh1", &mut rng).unwrap();
            let rolls = &result.groups[0].rolls;
            assert_eq!(result.total_f64(), rolls.iter().map(|r| r.value).max().unwrap() as f64);
            assert_eq!(rolls.iter().filter(|r| r.kept).count(), 1);
        }
    }

    #[test]
    fn keep_lowest_and_drop() {
        let mut rng = seeded(3);
        for _ in 0..100 {
            let result = roll_with("2d20kl1", &mut rng).unwrap();
            assert_eq!(result.total_f64(), result.groups[0].rolls.iter().map(|r| r.value).min().unwrap() as f64);
            let result = roll_with("4d6dl1", &mut rng).unwrap();
            let mut values: Vec<i64> = result.groups[0].rolls.iter().map(|r| r.value).collect();
            values.sort();
            assert_eq!(result.total_f64(), values[1..].iter().sum::<i64>() as f64);
        }
    }

    #[test]
    fn ties_drop_the_earlier_die() {
        let result = roll_with("2d20kh1", &mut Fixed::new(&[12, 12])).unwrap();
        let kept: Vec<bool> = result.groups[0].rolls.iter().map(|r| r.kept).collect();
        assert_eq!(kept, [true, false]);
        let result = roll_with("3d6dl1", &mut Fixed::new(&[2, 5, 2])).unwrap();
        let kept: Vec<bool> = result.groups[0].rolls.iter().map(|r| r.kept).collect();
        assert_eq!(kept, [false, true, true]);
    }

    #[test]
    fn shorthand_and_default_counts() {
        assert_eq!(roll_with("adv+5", &mut seeded(1)).unwrap().expression, "2d20kh1+5");
        assert_eq!(roll_with("dis", &mut seeded(1)).unwrap().expression, "2d20kl1");
        assert_eq!(roll_with("Advantage", &mut seeded(1)).unwrap().expression, "2d20kh1");
        let result = roll_with("2d20kh", &mut seeded(1)).unwrap();
        assert_eq!(result.groups[0].rolls.iter().filter(|r| r.kept).count(), 1);
    }

    #[test]
    fn exploding_dice_add_extra_rolls() {
        let result = roll_with("1d6!", &mut Fixed::new(&[6, 6, 2])).unwrap();
        assert_eq!(result.total, Value::from(14));
        let exploded: Vec<bool> = result.groups[0].rolls.iter().map(|r| r.exploded).collect();
        assert_eq!(exploded, [false, true, true]);
        assert_eq!(result.breakdown, "1d6! [6, 6!, 2!] = 14");
    }

    #[test]
    fn reroll_once() {
        // The 1 rerolls once into a 2 (kept even though it's still low); the 4 stays.
        let result = roll_with("2d6ro2", &mut Fixed::new(&[1, 2, 4])).unwrap();
        assert_eq!(result.total, Value::from(6));
        let discarded: Vec<bool> = result.groups[0].rolls.iter().map(|r| r.discarded).collect();
        assert_eq!(discarded, [true, false, false]);
    }

    #[test]
    fn reroll_until_higher() {
        let result = roll_with("1d6r2", &mut Fixed::new(&[1, 2, 5])).unwrap();
        assert_eq!(result.total, Value::from(5));
        assert_eq!(result.breakdown, "1d6r2 [~~1~~, ~~2~~, 5] = 5");
    }

    #[test]
    fn crit_and_fumble_detection() {
        assert!(roll_with("d20+5", &mut Always(20)).unwrap().critical);
        assert!(roll_with("d20+5", &mut Always(1)).unwrap().fumble);
        assert_eq!(roll_with("3d6", &mut Always(1)).unwrap().natural, None);
        assert_eq!(roll_with("2d20kh1", &mut Fixed::new(&[3, 20])).unwrap().natural, Some(20));
    }

    #[test]
    fn breakdown_marks_dropped_dice() {
        let result = roll_with("2d20kh1+5", &mut Fixed::new(&[4, 17])).unwrap();
        assert_eq!(result.breakdown, "2d20kh1 [~~4~~, 17] + 5 = 22");
        assert_eq!(roll_with("3", &mut Always(1)).unwrap().breakdown, "3");
        assert_eq!(roll_with("(1+2)*-3", &mut Always(1)).unwrap().breakdown, "(1 + 2) * -3 = -9");
        assert_eq!(roll_with("max(1, 2)", &mut Always(1)).unwrap().breakdown, "max(1, 2) = 2");
    }

    #[test]
    fn json_shape() {
        let result = roll_with("4dF", &mut Fixed::new(&[-1, 0, 1, 1])).unwrap();
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["groups"][0]["sides"], "F");
        assert_eq!(json["groups"][0]["rolls"][0], serde_json::json!({"value": -1, "kept": true}));
        assert_eq!(json["total"], 1);
        assert_eq!(serde_json::to_value(roll_with("7/2", &mut Always(1)).unwrap()).unwrap()["total"], 3.5);
    }

    #[test]
    fn decimals_print_like_python() {
        assert_eq!(format_number(3.0), "3");
        assert_eq!(format_number(-0.5), "-0.5");
        assert_eq!(format_number(1.0 / 3.0), "0.333333");
        assert_eq!(format_number(1234567.5), "1.23457e+06");
        assert_eq!(format_number(0.00001234), "1.234e-05");
        assert_eq!(format_number(1e15), "1e+15");
        assert_eq!(roll_with("10/4", &mut Always(1)).unwrap().breakdown, "10 / 4 = 2.5");
    }

    #[test]
    fn invalid_expressions_fail() {
        let long = "1d6!".repeat(200);
        let bad = [
            "",
            "2d",
            "d0",
            "1001d6",
            "5d6kh9",
            "1/0",
            "foo(2)",
            "2+",
            "3d6r6",
            "(1+2",
            "2d6kh1dl1",
            "import os",
            "__import__('os')",
            &long,
            "0^-1",
            "(-8)^0.5",
            "99999999999999999999d6",
        ];
        for expr in bad {
            assert!(roll(expr).is_err(), "{expr:?} should fail");
        }
    }

    #[test]
    fn error_messages() {
        let message = |expr: &str| roll(expr).unwrap_err().0;
        assert_eq!(message(""), "empty expression");
        assert_eq!(message("2+"), "unexpected end of expression");
        assert_eq!(message("(1+2"), "expected ')' at end of input");
        assert_eq!(message("foo(2)"), "unknown name 'foo'");
        assert_eq!(message("1 $ 2"), "unexpected character '$' at position 3");
        assert_eq!(message("1 2"), "unexpected '2' at position 3");
        assert_eq!(message("5d6kh9"), "can't keep 9 of 5 dice");
        assert_eq!(message("3d6r6"), "reroll threshold 6 would reroll every face of a d6");
        assert_eq!(message("round(1, 2, 3)"), "round() takes 1-2 argument(s)");
        assert_eq!(message("prof(0)"), "prof() takes a level from 1 to 30");
    }

    #[test]
    fn huge_exponent_rejected() {
        assert!(calculate("9^9^9").is_err());
    }

    #[test]
    fn roll_stats_gives_six_scores() {
        let stats = roll_stats_with(&mut seeded(1));
        assert_eq!(stats.len(), 6);
        assert!(stats.iter().all(|s| (3.0..=18.0).contains(&s.total_f64())));
    }
}
