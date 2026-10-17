use crate::config::InfoParseFn;
use regex::Regex;
use std::path::Path;

pub struct InfoResult {
    pub name: String,
    pub auth: String,
    pub rule_index: Option<usize>,
    pub used_fallback: bool,
}

/// File-name stem of a repo-relative path: "src/Naruto - Kishimoto.cbz" -> "Naruto - Kishimoto".
pub fn file_stem(rel_path: &str) -> String {
    Path::new(rel_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Parse a repo-relative path into (name, auth) using the configured rules.
///
/// The file-name stem (extension stripped) is split by the rule's `split`
/// delimiter into `arr`; the rule's `name`/`auth` templates are then rendered
/// against it. The first rule whose `name` renders non-empty wins; any extra
/// segments beyond what the templates reference are ignored, which covers
/// filenames carrying trailing info ("name - auth - vol1.suffix"). When no
/// rule applies, the whole stem becomes the name and auth stays empty.
pub fn parse_info(rel_path: &str, rules: &[InfoParseFn]) -> InfoResult {
    for (i, rule) in rules.iter().enumerate() {
        if let Some((name, auth)) = eval_rule(rel_path, rule) {
            return InfoResult {
                name,
                auth,
                rule_index: Some(i),
                used_fallback: false,
            };
        }
    }
    let stem = file_stem(rel_path).trim().to_string();
    let name = if stem.is_empty() {
        rel_path.to_string()
    } else {
        stem
    };
    InfoResult {
        name,
        auth: String::new(),
        rule_index: None,
        used_fallback: true,
    }
}

/// Render one rule against a path; `None` when the rule does not apply
/// (regex miss, empty name template, or a name that renders empty).
pub fn eval_rule(rel_path: &str, rule: &InfoParseFn) -> Option<(String, String)> {
    if let Some(pattern) = &rule.match_re {
        match Regex::new(pattern) {
            Ok(re) => {
                if !re.is_match(rel_path) {
                    return None;
                }
            }
            Err(_) => return None,
        }
    }
    if rule.name.trim().is_empty() {
        return None;
    }
    let stem = file_stem(rel_path);
    let parts: Vec<&str> = stem.split(rule.split.as_str()).collect();
    let name = eval_template(&rule.name, &parts).trim().to_string();
    if name.is_empty() {
        return None;
    }
    let auth = if rule.auth.trim().is_empty() {
        String::new()
    } else {
        eval_template(&rule.auth, &parts).trim().to_string()
    };
    Some((name, auth))
}

/// Every ${...} group in a template must parse.
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
    use crate::config::InfoParseFn;

    fn rule(split: &str, name: &str, auth: &str) -> InfoParseFn {
        InfoParseFn {
            match_re: None,
            split: split.to_string(),
            name: name.to_string(),
            auth: auth.to_string(),
        }
    }

    #[test]
    fn test_default_rule_name_auth() {
        // 默认预设：文件名形如 "name - auth.suffix"
        let rules = crate::config::default_info_parse_fns();
        let r = parse_info("manga/Naruto - Kishimoto.cbz", &rules);
        assert_eq!(r.name, "Naruto");
        assert_eq!(r.auth, "Kishimoto");
        assert_eq!(r.rule_index, Some(0));
        assert!(!r.used_fallback);
    }

    #[test]
    fn test_name_only_file() {
        // "name.suffix"：没有第二段，auth 为空
        let rules = crate::config::default_info_parse_fns();
        let r = parse_info("manga/Naruto.cbz", &rules);
        assert_eq!(r.name, "Naruto");
        assert_eq!(r.auth, "");
    }

    #[test]
    fn test_trailing_extra_info_ignored() {
        // 文件名后面还有其他附加信息时，只取前两段
        let rules = crate::config::default_info_parse_fns();
        let r = parse_info("manga/Naruto - Kishimoto - vol1.cbz", &rules);
        assert_eq!(r.name, "Naruto");
        assert_eq!(r.auth, "Kishimoto");
    }

    #[test]
    fn test_auth_first_preset() {
        // 另一套常用预设：auth 在前
        let rules = vec![rule(" - ", "${arr[1]}", "${arr[0]}")];
        let r = parse_info("manga/Kishimoto - Naruto.cbz", &rules);
        assert_eq!(r.name, "Naruto");
        assert_eq!(r.auth, "Kishimoto");
    }

    #[test]
    fn test_fallback_when_name_renders_empty() {
        // 规则的 name 渲染为空（越界索引）→ 跳过该规则，回退为完整文件名
        let rules = vec![rule(" - ", "${arr[5]}", "${arr[0]}")];
        let r = parse_info("src/utils/date-helper.ts", &rules);
        assert_eq!(r.name, "date-helper", "无规则产出 name 时回退为完整文件名");
        assert_eq!(r.auth, "");
        assert!(r.used_fallback);
        assert_eq!(r.rule_index, None);
    }

    #[test]
    fn test_fallback_when_no_rules_configured() {
        let r = parse_info("src/utils/date-helper.ts", &[]);
        assert_eq!(r.name, "date-helper");
        assert_eq!(r.auth, "");
        assert!(r.used_fallback);
        assert_eq!(r.rule_index, None);
    }

    #[test]
    fn test_regex_match_scopes_rule() {
        let rules = vec![InfoParseFn {
            match_re: Some(r"^manga/".to_string()),
            split: " - ".to_string(),
            name: "${arr[1]}".to_string(),
            auth: "${arr[0]}".to_string(),
        }];
        let r = parse_info("manga/Kishimoto - Naruto.cbz", &rules);
        assert_eq!(r.name, "Naruto");
        assert_eq!(r.auth, "Kishimoto");

        // 不在 match 范围内：回退
        let r2 = parse_info("src/Kishimoto - Naruto.cbz", &rules);
        assert_eq!(r2.name, "Kishimoto - Naruto");
        assert!(r2.used_fallback);
    }

    #[test]
    fn test_empty_name_template_skips_rule() {
        let rules = vec![rule(" - ", "", ""), rule(" - ", "${arr[0]}", "${arr[1]}")];
        let r = parse_info("a - b.cbz", &rules);
        assert_eq!(r.rule_index, Some(1), "name 模板为空的规则应被跳过");
        assert_eq!(r.name, "a");
    }

    #[test]
    fn test_bad_templates() {
        assert!(!template_ok("${arr[}"));
        assert!(!template_ok("${foo}"));
        assert!(template_ok("${arr[0]}"));
        assert!(template_ok("plain"));
    }
}
