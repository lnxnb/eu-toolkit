//! Minimal parser for Paradox/Clausewitz script files (`key = value`,
//! nested `{}` blocks, bare value lists, `#` comments, quoted strings).

#[derive(Debug, Clone)]
pub enum Value {
    Scalar(String),
    Block(Block),
}

#[derive(Debug, Clone, Default)]
pub struct Block {
    /// (key, value) pairs; a `None` key is a bare list element.
    pub items: Vec<(Option<String>, Value)>,
}

impl Block {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.items
            .iter()
            .find_map(|(k, v)| (k.as_deref() == Some(key)).then_some(v))
    }

    pub fn get_scalar(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Value::Scalar(s) => Some(s),
            _ => None,
        }
    }

    pub fn get_block(&self, key: &str) -> Option<&Block> {
        match self.get(key)? {
            Value::Block(b) => Some(b),
            _ => None,
        }
    }

    /// Bare (keyless) scalar items, e.g. the ids in `tropical = { 1 2 3 }`.
    pub fn bare_scalars(&self) -> impl Iterator<Item = &str> {
        self.items.iter().filter_map(|(k, v)| match (k, v) {
            (None, Value::Scalar(s)) => Some(s.as_str()),
            _ => None,
        })
    }

    pub fn bare_ids(&self) -> Vec<u32> {
        self.bare_scalars().filter_map(|s| s.parse().ok()).collect()
    }

    /// All `key = { ... }` items in order.
    pub fn key_blocks(&self) -> impl Iterator<Item = (&str, &Block)> {
        self.items.iter().filter_map(|(k, v)| match (k, v) {
            (Some(k), Value::Block(b)) => Some((k.as_str(), b)),
            _ => None,
        })
    }
}

enum Token {
    Open,
    Close,
    Eq,
    Sym(String),
}

pub fn parse(text: &str) -> Block {
    let tokens = tokenize(text);
    let mut pos = 0;
    parse_block(&tokens, &mut pos)
}

fn tokenize(text: &str) -> Vec<Token> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'#' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'{' => {
                out.push(Token::Open);
                i += 1;
            }
            b'}' => {
                out.push(Token::Close);
                i += 1;
            }
            b'=' => {
                out.push(Token::Eq);
                i += 1;
            }
            b'"' => {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += 1;
                }
                out.push(Token::Sym(
                    String::from_utf8_lossy(&bytes[start..i]).into_owned(),
                ));
                i += 1;
            }
            c if c.is_ascii_whitespace() => i += 1,
            _ => {
                let start = i;
                while i < bytes.len()
                    && !bytes[i].is_ascii_whitespace()
                    && !matches!(bytes[i], b'{' | b'}' | b'=' | b'#' | b'"')
                {
                    i += 1;
                }
                out.push(Token::Sym(
                    String::from_utf8_lossy(&bytes[start..i]).into_owned(),
                ));
            }
        }
    }
    out
}

fn parse_block(tokens: &[Token], pos: &mut usize) -> Block {
    let mut block = Block::default();
    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Close => {
                *pos += 1;
                return block;
            }
            Token::Open => {
                *pos += 1;
                let inner = parse_block(tokens, pos);
                block.items.push((None, Value::Block(inner)));
            }
            Token::Eq => {
                *pos += 1;
            }
            Token::Sym(s) => {
                if matches!(tokens.get(*pos + 1), Some(Token::Eq)) {
                    let key = s.clone();
                    *pos += 2;
                    match tokens.get(*pos) {
                        Some(Token::Open) => {
                            *pos += 1;
                            let inner = parse_block(tokens, pos);
                            block.items.push((Some(key), Value::Block(inner)));
                        }
                        Some(Token::Sym(v)) => {
                            block.items.push((Some(key), Value::Scalar(v.clone())));
                            *pos += 1;
                        }
                        _ => {}
                    }
                } else {
                    block.items.push((None, Value::Scalar(s.clone())));
                    *pos += 1;
                }
            }
        }
    }
    block
}

/// Reads `color = { ... }` contents: floats (0-1, detected by a decimal
/// point) are scaled to 0-255, integers are taken as-is.
pub fn color_from_block(block: &Block) -> Option<[u8; 3]> {
    let raw: Vec<&str> = block.bare_scalars().take(3).collect();
    if raw.len() < 3 {
        return None;
    }
    let is_float = raw.iter().any(|s| s.contains('.'));
    let mut rgb = [0u8; 3];
    for (i, s) in raw.iter().enumerate() {
        let v: f64 = s.parse().ok()?;
        let v = if is_float { v * 255.0 } else { v };
        rgb[i] = v.round().clamp(0.0, 255.0) as u8;
    }
    Some(rgb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_script() {
        let text = r#"
# comment
name = "My Mod"
owner = SWE
1450.1.1 = { owner = DAN } # dated block
tags = { 1 2 3 }
group = {
    catholic = {
        color = { 0.8 0.6 0.0 }
    }
}
"#;
        let b = parse(text);
        assert_eq!(b.get_scalar("name"), Some("My Mod"));
        assert_eq!(b.get_scalar("owner"), Some("SWE"));
        assert_eq!(b.get_block("tags").unwrap().bare_ids(), vec![1, 2, 3]);
        let dated = b.get_block("1450.1.1").unwrap();
        assert_eq!(dated.get_scalar("owner"), Some("DAN"));
        let catholic = b
            .get_block("group")
            .unwrap()
            .get_block("catholic")
            .unwrap();
        assert_eq!(
            color_from_block(catholic.get_block("color").unwrap()),
            Some([204, 153, 0])
        );
    }

    #[test]
    fn parses_integer_colors() {
        let b = parse("color = { 157 51 167 }");
        assert_eq!(
            color_from_block(b.get_block("color").unwrap()),
            Some([157, 51, 167])
        );
    }
}
