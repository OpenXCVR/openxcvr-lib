/// Evaluates simple integer arithmetic expressions that appear in page schema
/// offset_expr and bit_offset_expr fields (e.g. "86 + 4 * (app - 1)").
///
/// Grammar:
///   expr   := term (('+' | '-') term)*
///   term   := factor (('*' | '/' | '%') factor)*
///   factor := '-' factor | '(' expr ')' | integer | identifier
///
/// One variable is substituted before evaluation.
pub fn compute(expr: &str, var: &str, value: i64) -> Result<i64, String> {
    let tokens = tokenize(expr)?;
    let mut pos = 0;
    let result = parse_expr(&tokens, &mut pos, var, value)?;
    if pos != tokens.len() {
        return Err(format!("unexpected token at position {pos} in {expr:?}"));
    }
    Ok(result)
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Num(i64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LParen,
    RParen,
}

fn tokenize(s: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = s.chars().peekable();

    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' => {
                chars.next();
            }
            '0'..='9' => {
                let mut n = String::new();
                while chars.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                    n.push(chars.next().unwrap());
                }
                tokens.push(Token::Num(n.parse::<i64>().map_err(|e| e.to_string())?));
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let mut ident = String::new();
                while chars
                    .peek()
                    .map(|c| c.is_alphanumeric() || *c == '_')
                    .unwrap_or(false)
                {
                    ident.push(chars.next().unwrap());
                }
                tokens.push(Token::Ident(ident));
            }
            '+' => {
                tokens.push(Token::Plus);
                chars.next();
            }
            '-' => {
                tokens.push(Token::Minus);
                chars.next();
            }
            '*' => {
                tokens.push(Token::Star);
                chars.next();
            }
            '/' => {
                tokens.push(Token::Slash);
                chars.next();
            }
            '%' => {
                tokens.push(Token::Percent);
                chars.next();
            }
            '(' => {
                tokens.push(Token::LParen);
                chars.next();
            }
            ')' => {
                tokens.push(Token::RParen);
                chars.next();
            }
            other => return Err(format!("unexpected char {other:?}")),
        }
    }
    Ok(tokens)
}

fn parse_expr(tokens: &[Token], pos: &mut usize, var: &str, val: i64) -> Result<i64, String> {
    let mut result = parse_term(tokens, pos, var, val)?;
    loop {
        match tokens.get(*pos) {
            Some(Token::Plus) => {
                *pos += 1;
                result += parse_term(tokens, pos, var, val)?;
            }
            Some(Token::Minus) => {
                *pos += 1;
                result -= parse_term(tokens, pos, var, val)?;
            }
            _ => break,
        }
    }
    Ok(result)
}

fn parse_term(tokens: &[Token], pos: &mut usize, var: &str, val: i64) -> Result<i64, String> {
    let mut result = parse_factor(tokens, pos, var, val)?;
    loop {
        match tokens.get(*pos) {
            Some(Token::Star) => {
                *pos += 1;
                result *= parse_factor(tokens, pos, var, val)?;
            }
            Some(Token::Slash) => {
                *pos += 1;
                let d = parse_factor(tokens, pos, var, val)?;
                if d == 0 {
                    return Err("division by zero".into());
                }
                result /= d;
            }
            Some(Token::Percent) => {
                *pos += 1;
                let d = parse_factor(tokens, pos, var, val)?;
                if d == 0 {
                    return Err("modulo by zero".into());
                }
                result %= d;
            }
            _ => break,
        }
    }
    Ok(result)
}

fn parse_factor(tokens: &[Token], pos: &mut usize, var: &str, val: i64) -> Result<i64, String> {
    match tokens.get(*pos) {
        Some(Token::Minus) => {
            *pos += 1;
            Ok(-parse_factor(tokens, pos, var, val)?)
        }
        Some(Token::LParen) => {
            *pos += 1;
            let result = parse_expr(tokens, pos, var, val)?;
            match tokens.get(*pos) {
                Some(Token::RParen) => {
                    *pos += 1;
                    Ok(result)
                }
                _ => Err(format!("expected ')' at position {pos}")),
            }
        }
        Some(Token::Num(n)) => {
            let n = *n;
            *pos += 1;
            Ok(n)
        }
        Some(Token::Ident(name)) => {
            let name = name.clone();
            *pos += 1;
            if name == var {
                Ok(val)
            } else {
                Err(format!("unknown variable {name:?} (expected {var:?})"))
            }
        }
        _ => Err(format!("expected expression at position {pos}")),
    }
}

#[cfg(test)]
mod tests {
    use super::compute;

    #[test]
    fn test_simple_addition() {
        assert_eq!(compute("4 + bank", "bank", 2).unwrap(), 6);
    }

    #[test]
    fn test_linear_with_parens() {
        assert_eq!(compute("86 + 4 * (app - 1)", "app", 3).unwrap(), 94);
    }

    #[test]
    fn test_division() {
        assert_eq!(compute("(lane - 1) / 2", "lane", 3).unwrap(), 1);
        assert_eq!(compute("(lane - 1) / 2", "lane", 4).unwrap(), 1);
        assert_eq!(compute("(lane - 1) / 2", "lane", 5).unwrap(), 2);
    }

    #[test]
    fn test_modulo() {
        assert_eq!(compute("((lane - 1) % 2) * 4", "lane", 1).unwrap(), 0);
        assert_eq!(compute("((lane - 1) % 2) * 4", "lane", 2).unwrap(), 4);
        assert_eq!(compute("((lane - 1) % 2) * 4", "lane", 3).unwrap(), 0);
    }

    #[test]
    fn test_literal_only() {
        assert_eq!(compute("128", "x", 0).unwrap(), 128);
    }

    #[test]
    fn test_subtraction() {
        assert_eq!(compute("7 - severity", "severity", 3).unwrap(), 4);
    }
}
