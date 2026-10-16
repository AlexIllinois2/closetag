use crate::config::IdGenFn;
use regex::Regex;

pub struct IdResult {
    pub id: String,
    pub rule_index: Option<usize>,
    pub used_fallback: bool,
}

/// Derive an item id from a repo-relative path using the configured rules.
/// The first rule whose optional `match` regex hits and whose template renders
/// to a non-empty value wins; otherwise the full relative path is used.
pub fn generate_id(rel_path: &str, rules: &[IdGenFn]) -> IdResult {
    for (i, rule) in rules.iter().enumerate() {
        if let Some(pattern) = &rule.match_re {
            match Regex::new(pattern) {
                Ok(re) => {
                    if !re.is_match(rel_path) {
                        continue;
                    }
                }
                Err(_) => continue,
            }
        }
        if rule.template.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = rel_path.split(rule.split.as_str()).collect();
        let out = eval_template(&rule.template, &parts);
        if !out.trim().is_empty() {
            return IdResult {
                id: out,
                rule_index: Some(i),
                used_fallback: false,
            };
        }
    }
    IdResult {
        id: rel_path.to_string(),
        rule_index: None,
        used_fallback: true,
    }
}

/// Every ${...} group in the template must parse.
pub fn template_ok(template: &str) -> bool {
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        let after = &rest[start + 2..];
        let end = match after.find('}') {
            Some(e) => e,
            None => return false,
        };
        if parse_expr(&after[..end], &["x", "y"]).is_none() {
            return false;
        }
        rest = &after[end + 1..];
    }
    true
}

pub fn eval_template(template: &str, parts: &[&str]) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                out.push_str(&parse_expr(&after[..end], parts).unwrap_or_default());
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(after);
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Str(String),
    Arr(usize),
    Or,
    Plus,
    LParen,
    RParen,
}

fn parse_expr(expr: &str, parts: &[&str]) -> Option<String> {
    let toks = tokenize(expr)?;
    let mut pos = 0usize;
    let v = parse_or(&toks, &mut pos, parts)?;
    if pos == toks.len() {
        Some(v)
    } else {
        None
    }
}

fn tokenize(expr: &str) -> Option<Vec<Tok>> {
    let mut toks = Vec::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '\'' | '"' => {
                let quote = c;
                i += 1;
                let mut s = String::new();
                while i < chars.len() && chars[i] != quote {
                    s.push(chars[i]);
                    i += 1;
                }
                if i >= chars.len() {
                    return None;
                }
                i += 1;
                toks.push(Tok::Str(s));
            }
            '|' => {
                if i + 1 < chars.len() && chars[i + 1] == '|' {
                    toks.push(Tok::Or);
                    i += 2;
                } else {
                    return None;
                }
            }
            '+' => {
                toks.push(Tok::Plus);
                i += 1;
            }
            '(' => {
                toks.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                toks.push(Tok::RParen);
                i += 1;
            }
            _ => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                if word == "arr" {
                    if i >= chars.len() || chars[i] != '[' {
                        return None;
                    }
                    i += 1;
                    let ns = i;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                    if ns == i || i >= chars.len() || chars[i] != ']' {
                        return None;
                    }
                    let n: usize = chars[ns..i].iter().collect::<String>().parse().ok()?;
                    i += 1;
                    toks.push(Tok::Arr(n));
                } else {
                    return None;
                }
            }
        }
    }
    Some(toks)
}

fn parse_or(toks: &[Tok], pos: &mut usize, parts: &[&str]) -> Option<String> {
    let mut left = parse_cat(toks, pos, parts)?;
    while *pos < toks.len() && toks[*pos] == Tok::Or {
        *pos += 1;
        let right = parse_cat(toks, pos, parts)?;
        if left.trim().is_empty() {
            left = right;
        }
    }
    Some(left)
}

fn parse_cat(toks: &[Tok], pos: &mut usize, parts: &[&str]) -> Option<String> {
    let mut left = parse_term(toks, pos, parts)?;
    while *pos < toks.len() && toks[*pos] == Tok::Plus {
        *pos += 1;
        left.push_str(&parse_term(toks, pos, parts)?);
    }
    Some(left)
}

fn parse_term(toks: &[Tok], pos: &mut usize, parts: &[&str]) -> Option<String> {
    let t = toks.get(*pos)?.clone();
    *pos += 1;
    match t {
        Tok::Str(s) => Some(s),
        Tok::Arr(n) => Some(parts.get(n).copied().unwrap_or("").to_string()),
        Tok::LParen => {
            let v = parse_or(toks, pos, parts)?;
            if toks.get(*pos)? == &Tok::RParen {
                *pos += 1;
                Some(v)
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::IdGenFn;

    fn rule(split: &str, template: &str) -> IdGenFn {
        IdGenFn {
            match_re: None,
            split: split.to_string(),
            template: template.to_string(),
        }
    }

    #[test]
    fn test_default_template() {
        let rules = vec![rule("-", "${arr[0]||arr[1]}")];
        let r = generate_id("src/utils/date-helper.ts", &rules);
        assert_eq!(r.id, "src/utils/date");
        assert_eq!(r.rule_index, Some(0));
        assert!(!r.used_fallback);

        let r = generate_id("README.md", &rules);
        assert_eq!(r.id, "README.md");
    }

    #[test]
    fn test_fallback_when_empty() {
        let rules = vec![rule("-", "${arr[9]}")];
        let r = generate_id("a/b-c.ts", &rules);
        assert_eq!(r.id, "a/b-c.ts");
        assert!(r.used_fallback);
    }

    #[test]
    fn test_regex_match() {
        let rules = vec![IdGenFn {
            match_re: Some(r"\.md$".to_string()),
            split: "/".to_string(),
            template: "${arr[1]||arr[0]}".to_string(),
        }];
        let r = generate_id("docs/readme.md", &rules);
        assert_eq!(r.id, "readme.md");
        let r2 = generate_id("src/main.rs", &rules);
        assert_eq!(r2.id, "src/main.rs");
        assert!(r2.used_fallback);
    }

    #[test]
    fn test_bad_templates() {
        assert!(!template_ok("${arr[}"));
        assert!(!template_ok("${foo}"));
        assert!(template_ok("${arr[0]}"));
        assert!(template_ok("plain"));
    }
}
