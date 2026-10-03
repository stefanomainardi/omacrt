//! The record an arcade game keeps, read back out of the file its emulator
//! saves, the way a cabinet showed its top score in attract mode.
//!
//! FinalBurn Neo and MAME both save a game's high score table on exit, using
//! the memory ranges in MAME's `hiscore.dat`. What they save is a dump of the
//! game's own memory, laid out differently by every game: binary or BCD,
//! either byte order, scores stored divided by ten. The hi2txt project
//! (GreatStone, GPL-2.0) describes that layout for thousands of games, one
//! XML file each. Those files are not shipped here; the one for a set is
//! fetched the first time that set is looked at, like a cover, and kept.
//!
//! Only the part of hi2txt's format that carries most games is understood:
//! a structure picked by the size of the file, `elt` and `loop`, integers in
//! binary or BCD with byte and nibble skipping, and the score formats that
//! multiply, divide, add or append zeros. A definition that asks for anything
//! else gives no record at all. Measured against hi2txt's own outputs for the
//! 751 dumps in its test suite, this subset gave the right top score for 469
//! of them, a wrong one for 9 (two of which hi2txt reads out of a second file
//! this does not look at) and nothing for the rest, before the rule below that
//! a record of zero, or of a billion or more, is no record.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ------------------------------------------------------------------- xml

/// One element of a hi2txt file: enough XML for files written by hand in
/// one consistent style, and no more.
#[derive(Debug, Default)]
pub struct Node {
    pub tag: String,
    attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
    pub text: String,
}

impl Node {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    fn all(&self, tag: &str) -> impl Iterator<Item = &Node> {
        self.children.iter().filter(move |c| c.tag == tag)
    }

    fn first(&self, tag: &str) -> Option<&Node> {
        self.all(tag).next()
    }

    /// Every element under this one, depth first.
    fn walk<'a>(&'a self, out: &mut Vec<&'a Node>) {
        for c in &self.children {
            out.push(c);
            c.walk(out);
        }
    }
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let end = tail.find(';').filter(|&e| e <= 8);
        let (rep, used) = match end.map(|e| &tail[..=e]) {
            Some("&lt;") => ("<", 4),
            Some("&gt;") => (">", 4),
            Some("&amp;") => ("&", 5),
            Some("&quot;") => ("\"", 6),
            Some("&apos;") => ("'", 6),
            Some(other) => (" ", other.len()),
            None => ("&", 1),
        };
        out.push_str(rep);
        rest = &tail[used..];
    }
    out.push_str(rest);
    out
}

/// Parse a document into its root element.
pub fn parse(xml: &str) -> Option<Node> {
    let mut stack: Vec<Node> = vec![Node::default()];
    let mut rest = xml;
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            break;
        };
        if let Some(top) = stack.last_mut() {
            top.text.push_str(&unescape(&rest[..lt]));
        }
        rest = &rest[lt..];
        if let Some(r) = rest.strip_prefix("<!--") {
            rest = &r[r.find("-->")? + 3..];
        } else if rest.starts_with("<?") || rest.starts_with("<!") {
            rest = &rest[rest.find('>')? + 1..];
        } else if let Some(r) = rest.strip_prefix("</") {
            rest = &r[r.find('>')? + 1..];
            let done = stack.pop()?;
            stack.last_mut()?.children.push(done);
        } else {
            let end = rest.find('>')?;
            let inner = &rest[1..end];
            rest = &rest[end + 1..];
            let closed = inner.ends_with('/');
            let inner = inner.trim_end_matches('/');
            let mut parts = inner.splitn(2, char::is_whitespace);
            let tag = parts.next()?.to_string();
            let mut node = Node {
                tag,
                ..Default::default()
            };
            let mut a = parts.next().unwrap_or("");
            while let Some(eq) = a.find('=') {
                let key = a[..eq].trim().to_string();
                let after = a[eq + 1..].trim_start();
                let quote = after.chars().next()?;
                if quote != '"' && quote != '\'' {
                    return None;
                }
                let close = after[1..].find(quote)? + 1;
                node.attrs.push((key, unescape(&after[1..close])));
                a = &after[close + 1..];
            }
            if closed {
                stack.last_mut()?.children.push(node);
            } else {
                stack.push(node);
            }
        }
    }
    let mut root = stack.into_iter().next()?;
    root.children.retain(|c| !c.tag.is_empty());
    root.children.into_iter().find(|c| c.tag == "hi2txt")
}

// -------------------------------------------------------------- decoding

const ELT_ATTRS: [&str; 8] = [
    "size",
    "type",
    "id",
    "base",
    "endianness",
    "nibble-skip",
    "byte-skip",
    "byte-swap",
];

/// A record past this is a misreading, not a score anyone made.
const RECORD_MAX: f64 = 999_999_999.0;

struct Unsupported;

fn decode_int(bytes: &[u8], e: &Node) -> Result<Option<u64>, Unsupported> {
    if e.attrs
        .iter()
        .any(|(k, _)| !ELT_ATTRS.contains(&k.as_str()))
    {
        return Err(Unsupported);
    }
    let mut b = bytes.to_vec();
    if matches!(e.attr("byte-swap"), Some("yes" | "true")) {
        for pair in b.as_chunks_mut::<2>().0 {
            pair.swap(0, 1);
        }
    }
    b = match e.attr("byte-skip") {
        Some("odd") => b.iter().step_by(2).copied().collect(),
        Some("even") => b.iter().skip(1).step_by(2).copied().collect(),
        _ => b,
    };
    if e.attr("endianness") == Some("little_endian") {
        b.reverse();
    }
    let mut nibbles: Vec<u8> = b.iter().flat_map(|x| [x >> 4, x & 0xf]).collect();
    nibbles = match e.attr("nibble-skip") {
        Some("odd") => nibbles.iter().skip(1).step_by(2).copied().collect(),
        Some("even") => nibbles.iter().step_by(2).copied().collect(),
        _ => nibbles,
    };
    if e.attr("base") == Some("16") {
        // Binary coded decimal: each nibble one digit. A nibble above nine
        // is no digit, and no score.
        if nibbles.iter().any(|&n| n > 9) || nibbles.len() > 18 {
            return Ok(None);
        }
        Ok(Some(nibbles.iter().fold(0, |acc, &n| acc * 10 + n as u64)))
    } else {
        if nibbles.len() > 16 {
            return Ok(None);
        }
        Ok(Some(nibbles.iter().fold(0, |acc, &n| acc << 4 | n as u64)))
    }
}

type Rows = HashMap<usize, HashMap<String, Option<u64>>>;

fn walk_structure(
    items: &[Node],
    data: &[u8],
    mut pos: usize,
    row: Option<usize>,
    rows: &mut Rows,
    single: &mut HashMap<String, Option<u64>>,
) -> Result<usize, Unsupported> {
    for it in items {
        match it.tag.as_str() {
            "elt" => {
                let size: usize = it.attr("size").and_then(|s| s.parse().ok()).unwrap_or(1);
                let Some(chunk) = data.get(pos..pos + size) else {
                    return Ok(data.len());
                };
                pos += size;
                if it.attr("type").unwrap_or("int") == "int" {
                    let v = decode_int(chunk, it)?;
                    let id = it.attr("id").unwrap_or("").to_string();
                    match row {
                        Some(r) => {
                            rows.entry(r).or_default().insert(id, v);
                        }
                        None => {
                            single.entry(id).or_insert(v);
                        }
                    }
                }
            }
            "loop" => {
                if it.attrs.iter().any(|(k, _)| k != "count" && k != "start") {
                    return Err(Unsupported);
                }
                let count: usize = it.attr("count").and_then(|s| s.parse().ok()).unwrap_or(1);
                let start: usize = it.attr("start").and_then(|s| s.parse().ok()).unwrap_or(0);
                for i in 0..count {
                    let r = start + i;
                    let r = row.map_or(r, |outer| outer * 1000 + r);
                    pos = walk_structure(&it.children, data, pos, Some(r), rows, single)?;
                }
            }
            _ => {}
        }
    }
    Ok(pos)
}

fn structure_size(items: &[Node]) -> usize {
    items
        .iter()
        .map(|it| match it.tag.as_str() {
            "elt" => it.attr("size").and_then(|s| s.parse().ok()).unwrap_or(1),
            "loop" => {
                it.attr("count").and_then(|s| s.parse().ok()).unwrap_or(1)
                    * structure_size(&it.children)
            }
            _ => 0,
        })
        .sum()
}

/// The structure that describes a dump of this size, if one does.
fn pick(def: &Node, size: usize) -> Option<&Node> {
    let hi: Vec<&Node> = def
        .all("structure")
        .filter(|s| s.attr("file").unwrap_or(".hi") == ".hi")
        .collect();
    hi.iter()
        .find(|s| {
            s.first("check")
                .and_then(|c| c.first("size"))
                .and_then(|n| n.text.trim().parse::<usize>().ok())
                == Some(size)
        })
        .or_else(|| hi.iter().find(|s| structure_size(&s.children) == size))
        .copied()
}

/// Apply a column's formats to a score, or say they cannot be.
fn format(def: &Node, names: &str, mut v: f64) -> Option<f64> {
    for f in names
        .split([',', ';'])
        .map(str::trim)
        .filter(|f| !f.is_empty())
    {
        if f.starts_with("Pad") || f.starts_with("Trim") {
            continue;
        }
        if let Some(n) = f.strip_prefix('*').and_then(|n| n.parse::<f64>().ok()) {
            v *= n;
            continue;
        }
        if let Some(n) = f.strip_prefix('/').and_then(|n| n.parse::<f64>().ok()) {
            v /= n;
            continue;
        }
        let d = def.all("format").find(|d| d.attr("id") == Some(f))?;
        for op in &d.children {
            let n = op.text.trim();
            match op.tag.as_str() {
                "multiply" => v *= n.parse::<f64>().ok()?,
                "divide" => v /= n.parse::<f64>().ok()?,
                "add" => v += n.parse::<f64>().ok()?,
                "suffix" if !n.is_empty() && n.chars().all(|c| c == '0') => {
                    v *= 10f64.powi(n.len() as i32)
                }
                "trim" | "pad" => {}
                _ => return None,
            }
        }
    }
    Some(v)
}

/// The top score in a dump, from the game's hi2txt definition.
pub fn top_score(def: &Node, data: &[u8]) -> Option<u64> {
    let s = pick(def, data.len())?;
    let (mut rows, mut single) = (Rows::new(), HashMap::new());
    walk_structure(&s.children, data, 0, None, &mut rows, &mut single).ok()?;
    // A structure may name the output that goes with it; the rest share the
    // one with no name.
    let outputs: Vec<&Node> = def.all("output").collect();
    let out = match s.attr("output") {
        Some(want) => outputs.iter().find(|o| o.attr("id") == Some(want)).copied(),
        None => outputs
            .iter()
            .find(|o| o.attr("id").is_none())
            .or(outputs.first())
            .copied(),
    }?;
    let mut nodes = Vec::new();
    out.walk(&mut nodes);
    // Rows a table says to leave out, when it says so in the one way this
    // reads: a score of exactly, or more than, some value.
    let mut ignore: Option<(u64, bool)> = None;
    for t in nodes.iter().filter(|n| n.tag == "table") {
        if let Some(rule) = t.attr("line-ignore") {
            let v = rule.strip_prefix("SCORE:")?.parse::<u64>().ok()?;
            let more = match t.attr("line-ignore-operator").unwrap_or("=") {
                "=" => false,
                ">" => true,
                _ => return None,
            };
            ignore = Some((v, more));
        }
    }
    let scores: Vec<&&Node> = nodes
        .iter()
        .filter(|n| {
            (n.tag == "column" || n.tag == "field")
                && n.attr("id")
                    .is_some_and(|id| id.to_ascii_uppercase().contains("SCORE"))
        })
        .collect();
    let exact: Vec<&&Node> = scores
        .iter()
        .filter(|n| {
            n.attr("id")
                .is_some_and(|id| id.eq_ignore_ascii_case("SCORE"))
        })
        .copied()
        .collect();
    let cols = if exact.is_empty() { scores } else { exact };
    let col = cols.first()?;
    let id = col.attr("id")?;
    let src = col.attr("src").unwrap_or(id);
    let fmt = col.attr("format").unwrap_or("");
    let mut best: Option<f64> = None;
    let mut consider = |raw: Option<u64>| -> Option<()> {
        let Some(raw) = raw else {
            return Some(());
        };
        if let Some((v, more)) = ignore
            && ((!more && raw == v) || (more && raw > v))
        {
            return Some(());
        }
        let v = format(def, fmt, raw as f64)?;
        best = Some(best.map_or(v, |b: f64| b.max(v)));
        Some(())
    };
    for r in rows.values() {
        if let Some(raw) = r.get(src) {
            consider(*raw)?;
        }
    }
    if let Some(raw) = single.get(src) {
        consider(*raw)?;
    }
    best.filter(|&v| (1.0..RECORD_MAX).contains(&v))
        .map(|v| v as u64)
}

// ------------------------------------------------------- files and cache

const DEFINITIONS: &str =
    "https://raw.githubusercontent.com/GreatStoneEx/hi2txt-xml/master/src/main/db";

fn cache_dir() -> PathBuf {
    crate::crt::home().join(".cache/omacrt/hiscore")
}

/// A set's definition, from the cache or fetched once, following the
/// `sameas` a clone carries to its parent's.
pub fn definition(set: &str) -> Option<Node> {
    let mut name = set.to_string();
    for _ in 0..4 {
        if !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') || name.is_empty() {
            return None;
        }
        let text = cached_or_fetched(&name)?;
        let def = parse(&text)?;
        match def.first("sameas").and_then(|s| s.attr("id")) {
            Some(other) => name = other.to_string(),
            None => return Some(def),
        }
    }
    None
}

fn cached_or_fetched(name: &str) -> Option<String> {
    let dir = cache_dir();
    let file = dir.join(format!("{name}.xml"));
    if let Ok(text) = std::fs::read_to_string(&file) {
        return Some(text);
    }
    // A set hi2txt does not describe is asked for again after a month, not
    // every time its list is opened.
    let none = dir.join(format!("{name}.none"));
    if std::fs::metadata(&none)
        .and_then(|m| m.modified())
        .is_ok_and(|t| t.elapsed().is_ok_and(|e| e.as_secs() < 30 * 86_400))
    {
        return None;
    }
    let out = crate::net::curl(20, 1_048_576)
        .arg(format!("{DEFINITIONS}/{name}.xml"))
        .output()
        .ok()?;
    let _ = std::fs::create_dir_all(&dir);
    if !out.status.success() || out.stdout.is_empty() {
        let _ = std::fs::write(&none, b"");
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let _ = crate::store::save(&file, &text);
    Some(text)
}

/// Where the emulators keep a set's dump: MAME's hiscore folder and
/// FinalBurn Neo's saves. The newer of the two is the one last played.
pub fn dump_path(set: &str) -> Option<PathBuf> {
    let home = crate::crt::home();
    let candidates = [
        crate::coredata::system_dir()
            .join("mame/hiscore")
            .join(format!("{set}.hi")),
        home.join(".config/retroarch/saves/FinalBurn Neo/fbneo")
            .join(format!("{set}.hi")),
    ];
    candidates
        .into_iter()
        .filter_map(|p| {
            let t = std::fs::metadata(&p).and_then(|m| m.modified()).ok()?;
            Some((t, p))
        })
        .max_by_key(|(t, _)| *t)
        .map(|(_, p)| p)
}

/// The record a set's cabinet holds, if it has one and it can be read.
pub fn record(set: &str) -> Option<u64> {
    let path = dump_path(set)?;
    record_of(set, &path)
}

fn record_of(set: &str, path: &Path) -> Option<u64> {
    let data = std::fs::read(path).ok()?;
    top_score(&definition(set)?, &data)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A definition in hi2txt's style, written for these tests: five scores
    /// in BCD, little endian, three bytes each, then their names.
    const FIVE: &str = r#"<?xml version="1.0" encoding="utf-8" standalone="no"?>
<!DOCTYPE hi2txt SYSTEM "hi2txt.dtd">
<hi2txt>
  <!-- a comment, with <angle brackets> in it //-->
  <structure file=".hi">
    <check><size>30</size></check>
    <loop count="5">
      <elt size="3" type="int"  id="SCORE" endianness="little_endian" base="16"/>
    </loop>
    <loop count="5">
      <elt size="3" type="text" id="NAME"/>
    </loop>
  </structure>
  <output>
    <table>
      <column id="RANK" src="index" format="+1"/>
      <column id="SCORE" format="*10"/>
      <column id="NAME"/>
    </table>
  </output>
</hi2txt>"#;

    fn dump(scores: [u32; 5]) -> Vec<u8> {
        let mut out = Vec::new();
        for s in scores {
            // BCD, low byte first.
            let d = format!("{s:06}");
            let b: Vec<u8> = (0..3)
                .map(|i| u8::from_str_radix(&d[i * 2..i * 2 + 2], 16).unwrap())
                .collect();
            out.extend(b.iter().rev());
        }
        out.extend(b"AAABBBCCCDDDEEE");
        out
    }

    #[test]
    fn the_top_score_is_read_and_formatted() {
        let def = parse(FIVE).unwrap();
        assert_eq!(
            top_score(&def, &dump([1200, 3000, 2500, 800, 10])),
            Some(30000)
        );
    }

    #[test]
    fn a_dump_of_another_size_gives_no_record() {
        let def = parse(FIVE).unwrap();
        let mut d = dump([1200, 3000, 2500, 800, 10]);
        d.push(0);
        assert_eq!(top_score(&def, &d), None);
    }

    #[test]
    fn an_empty_table_is_no_record() {
        let def = parse(FIVE).unwrap();
        assert_eq!(top_score(&def, &dump([0; 5])), None);
    }

    #[test]
    fn a_definition_beyond_the_subset_gives_nothing_rather_than_a_guess() {
        let odd = FIVE.replace(r#"base="16"/>"#, r#"base="16" decoding-profile="bcd-le"/>"#);
        let def = parse(&odd).unwrap();
        assert_eq!(top_score(&def, &dump([1200, 3000, 2500, 800, 10])), None);
    }

    #[test]
    fn a_binary_score_and_a_divided_one() {
        let xml = r#"<hi2txt><structure><elt size="2" type="int" id="SCORE"/></structure>
            <output><field id="SCORE" format="halve"/></output>
            <format id="halve"><divide>2</divide></format></hi2txt>"#;
        let def = parse(xml).unwrap();
        assert_eq!(top_score(&def, &[0x01, 0x00]), Some(128));
    }

    #[test]
    fn a_clone_points_at_its_parent() {
        let def = parse(r#"<hi2txt><sameas id="puckman"/></hi2txt>"#).unwrap();
        assert_eq!(
            def.first("sameas").and_then(|s| s.attr("id")),
            Some("puckman")
        );
    }

    #[test]
    fn entities_and_unknown_ones_do_not_break_the_reader() {
        assert_eq!(unescape("a &lt; b &copy; c"), "a < b   c");
        let def = parse(r#"<hi2txt><output><field id="SCORE &amp; MORE"/></output></hi2txt>"#);
        assert!(def.is_some());
    }
}
