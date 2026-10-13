//! Block-style YAML read as a tree of lines, so every key keeps its line number (`render.yaml`, compose files).
// Note: block mappings and lists only; flow mappings, anchors and multi-document files read as plain
// text. Upgrade path: a parser with markers (saphyr) once a real manifest needs more.

#[derive(Debug, Default)]
pub struct Node {
    pub line: usize,
    pub key: String,
    pub value: String,
    pub item: bool,
    pub children: Vec<Node>,
}

impl Node {
    pub fn get(&self, key: &str) -> Option<&Node> {
        self.children.iter().find(|c| !c.item && c.key == key)
    }

    /// A child's scalar value, or None when it is missing or empty.
    pub fn str(&self, key: &str) -> Option<&str> {
        self.get(key).map(|n| n.value.as_str()).filter(|v| !v.is_empty())
    }

    pub fn items(&self) -> impl Iterator<Item = &Node> {
        self.children.iter().filter(|c| c.item)
    }
}

struct Entry {
    indent: usize,
    line: usize,
    item: bool,
    text: String,
}

pub fn parse(text: &str) -> Node {
    let mut entries = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = strip_comment(raw);
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed == "---" || trimmed == "..." {
            continue;
        }
        let mut indent = line.len() - trimmed.len();
        let mut rest = trimmed;
        while rest == "-" || rest.starts_with("- ") {
            entries.push(Entry {
                indent,
                line: i + 1,
                item: true,
                text: String::new(),
            });
            let after = rest[1..].trim_start();
            indent += rest.len() - after.len();
            rest = after;
        }
        if !rest.is_empty() {
            entries.push(Entry {
                indent,
                line: i + 1,
                item: false,
                text: rest.trim_end().to_string(),
            });
        }
    }
    let mut pos = 0;
    let children = block(&entries, &mut pos, entries.first().map_or(0, |e| e.indent));
    Node {
        children,
        ..Node::default()
    }
}

fn block(e: &[Entry], pos: &mut usize, indent: usize) -> Vec<Node> {
    let mut out = Vec::new();
    while *pos < e.len() && e[*pos].indent == indent {
        out.push(one(e, pos, indent));
    }
    // Lines indented deeper than their siblings with no parent to hold them are skipped, not misread.
    while *pos < e.len() && e[*pos].indent > indent {
        *pos += 1;
    }
    out
}

fn one(e: &[Entry], pos: &mut usize, indent: usize) -> Node {
    let en = &e[*pos];
    *pos += 1;
    let mut node = if en.item {
        Node {
            line: en.line,
            item: true,
            ..Node::default()
        }
    } else {
        split(en)
    };
    if *pos < e.len() && e[*pos].indent > indent {
        let deeper = e[*pos].indent;
        node.children = block(e, pos, deeper);
    } else if !node.item && node.value.is_empty() {
        // `key:` followed by `- item` lines at the same indent.
        while *pos < e.len() && e[*pos].indent == indent && e[*pos].item {
            node.children.push(one(e, pos, indent));
        }
    }
    node
}

fn split(en: &Entry) -> Node {
    let t = &en.text;
    let (key, value) = match find_colon(t) {
        Some(i) => (unquote(&t[..i]), unquote(t[i + 1..].trim())),
        None => (String::new(), unquote(t)),
    };
    Node {
        line: en.line,
        key,
        value,
        ..Node::default()
    }
}

/// The `:` that ends a key: followed by a space or the end of the line, outside quotes.
fn find_colon(t: &str) -> Option<usize> {
    let mut quote = None;
    for (i, c) in t.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, ':') if t[i + 1..].is_empty() || t[i + 1..].starts_with(' ') => return Some(i),
            _ => {}
        }
    }
    None
}

fn strip_comment(line: &str) -> &str {
    let mut quote = None;
    let mut prev = ' ';
    for (i, c) in line.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '#') if prev == ' ' || prev == '\t' => return &line[..i],
            _ => {}
        }
        prev = c;
    }
    line
}

pub fn unquote(s: &str) -> String {
    let s = s.trim();
    let b = s.as_bytes();
    if b.len() >= 2 && (b[0] == b'"' || b[0] == b'\'') && b[b.len() - 1] == b[0] {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// A command written as a scalar or as a flow list (`["npm", "start"]`), as one line.
pub fn command(value: &str) -> String {
    let v = value.trim();
    match v.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
        Some(list) => list
            .split(',')
            .map(unquote)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
        None => unquote(v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_lines_and_nesting() {
        let root =
            parse("services:\n  - type: web # the site\n    name: a\n    envVars:\n    - key: X\n  - type: redis\n");
        let services: Vec<_> = root.get("services").unwrap().items().collect();
        assert_eq!(services.len(), 2);
        assert_eq!(services[0].line, 2);
        assert_eq!(services[0].str("type"), Some("web"));
        assert_eq!(
            services[0].get("envVars").unwrap().items().next().unwrap().str("key"),
            Some("X")
        );
        assert_eq!(services[1].str("type"), Some("redis"));
        assert_eq!(command(r#"["npm", "run", "start"]"#), "npm run start");
    }
}
