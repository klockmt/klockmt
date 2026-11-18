#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    Str(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(fields) => fields
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(flag) => Some(*flag),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Json::Number(value) if *value >= 0.0 => Some(*value as u64),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(items) => Some(items),
            _ => None,
        }
    }
}

pub fn parse(text: &str) -> Result<Json, String> {
    let mut parser = Parser { text, position: 0 };
    let value = parser.value()?;
    parser.skip_whitespace();
    if parser.position != text.len() {
        return Err(format!("trailing data at byte {}", parser.position));
    }
    Ok(value)
}

struct Parser<'a> {
    text: &'a str,
    position: usize,
}

impl Parser<'_> {
    fn value(&mut self) -> Result<Json, String> {
        self.skip_whitespace();
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => self.string().map(Json::Str),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(format!("unexpected input at byte {}", self.position)),
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.expect(b'{')?;
        let mut fields = Vec::new();
        self.skip_whitespace();
        if self.consume(b'}') {
            return Ok(Json::Object(fields));
        }
        loop {
            self.skip_whitespace();
            let key = self.string()?;
            self.skip_whitespace();
            self.expect(b':')?;
            fields.push((key, self.value()?));
            self.skip_whitespace();
            if self.consume(b'}') {
                return Ok(Json::Object(fields));
            }
            self.expect(b',')?;
        }
    }

    fn array(&mut self) -> Result<Json, String> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.consume(b']') {
            return Ok(Json::Array(items));
        }
        loop {
            items.push(self.value()?);
            self.skip_whitespace();
            if self.consume(b']') {
                return Ok(Json::Array(items));
            }
            self.expect(b',')?;
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let run_start = self.position;
            while matches!(self.peek(), Some(byte) if byte != b'"' && byte != b'\\') {
                self.position += 1;
            }
            out.push_str(&self.text[run_start..self.position]);
            match self.next_byte()? {
                b'"' => return Ok(out),
                _ => out.push(self.escape()?),
            }
        }
    }

    fn escape(&mut self) -> Result<char, String> {
        match self.next_byte()? {
            b'"' => Ok('"'),
            b'\\' => Ok('\\'),
            b'/' => Ok('/'),
            b'b' => Ok('\u{8}'),
            b'f' => Ok('\u{c}'),
            b'n' => Ok('\n'),
            b'r' => Ok('\r'),
            b't' => Ok('\t'),
            b'u' => self.unicode_escape(),
            _ => Err(format!("invalid escape at byte {}", self.position)),
        }
    }

    fn unicode_escape(&mut self) -> Result<char, String> {
        let end = self.position + 4;
        let digits = self
            .text
            .get(self.position..end)
            .ok_or_else(|| "truncated unicode escape".to_owned())?;
        let code = u32::from_str_radix(digits, 16)
            .map_err(|_| format!("invalid unicode escape {digits}"))?;
        self.position = end;
        Ok(char::from_u32(code).unwrap_or('\u{FFFD}'))
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.position;
        while matches!(
            self.peek(),
            Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
        ) {
            self.position += 1;
        }
        self.text[start..self.position]
            .parse::<f64>()
            .map(Json::Number)
            .map_err(|_| format!("invalid number at byte {start}"))
    }

    fn literal(&mut self, word: &str, value: Json) -> Result<Json, String> {
        if self.text[self.position..].starts_with(word) {
            self.position += word.len();
            return Ok(value);
        }
        Err(format!("expected {word} at byte {}", self.position))
    }

    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.position).copied()
    }

    fn next_byte(&mut self) -> Result<u8, String> {
        let byte = self
            .peek()
            .ok_or_else(|| "unexpected end of input".to_owned())?;
        self.position += 1;
        Ok(byte)
    }

    fn consume(&mut self, expected: u8) -> bool {
        if self.peek() == Some(expected) {
            self.position += 1;
            return true;
        }
        false
    }

    fn expect(&mut self, expected: u8) -> Result<(), String> {
        if self.consume(expected) {
            return Ok(());
        }
        Err(format!(
            "expected '{}' at byte {}",
            char::from(expected),
            self.position
        ))
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.position += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_objects_arrays_and_escapes() {
        let parsed = parse(r#"{"name":"a\"bç","items":[1,true,null],"ok":false}"#).unwrap();
        assert_eq!(parsed.get("name").and_then(Json::as_str), Some("a\"bç"));
        assert_eq!(
            parsed
                .get("items")
                .and_then(Json::as_array)
                .map(<[Json]>::len),
            Some(3)
        );
        assert_eq!(parsed.get("ok").and_then(Json::as_bool), Some(false));
    }

    #[test]
    fn keeps_raw_utf8_text() {
        let parsed = parse(r#"["relatório"]"#).unwrap();
        assert_eq!(
            parsed.as_array().and_then(|items| items[0].as_str()),
            Some("relatório")
        );
    }

    #[test]
    fn accepts_pretty_printed_json_like_github_responses() {
        let pretty = "{\n  \"message\": \"Bad credentials\",\n  \"items\": [\n    1 ,\n    2\n  ],\n  \"parents\": [\n\n  ]\n}";
        let parsed = parse(pretty).unwrap();
        assert_eq!(
            parsed.get("message").and_then(Json::as_str),
            Some("Bad credentials")
        );
        assert_eq!(
            parsed
                .get("items")
                .and_then(Json::as_array)
                .map(<[Json]>::len),
            Some(2)
        );
    }

    #[test]
    fn rejects_trailing_data() {
        assert!(parse("{} x").is_err());
    }

    #[test]
    fn rejects_truncated_input() {
        assert!(parse(r#"{"a":"#).is_err());
    }
}
