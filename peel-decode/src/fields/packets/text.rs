//! Text components: a JSON string up to 1.20.2, one network NBT tag from
//! 1.20.3 (`ComponentSerialization`). Shown as plain text, with the tag's
//! tree or the raw JSON beneath.

use super::super::reader::Result;
use super::super::{Decoder, Name};
use super::V1_20_3;
use super::nbt::{self, Value};

pub fn component(d: &mut Decoder, name: impl Into<Name>) -> Result<()> {
    if d.protocol() < V1_20_3 {
        return json_component(d, name);
    }
    let value = nbt::root(d, name)?;
    show_plain(d, &value);
    Ok(())
}

/// A component as JSON text, which `login_disconnect` sends in every version.
pub fn json_component(d: &mut Decoder, name: impl Into<Name>) -> Result<()> {
    let value = d.group(name, |d| {
        let json = d.string("json")?;
        Ok(serde_json::from_str(json).map_or(Value::Other, from_json))
    })?;
    show_plain(d, &value);
    Ok(())
}

fn show_plain(d: &mut Decoder, value: &Value) {
    d.show_last(format!("{:?}", plain(value)));
}

pub fn optional_component(d: &mut Decoder, name: impl Into<Name>) -> Result<()> {
    d.optional(name, component).map(drop)
}

fn from_json(json: serde_json::Value) -> Value {
    match json {
        serde_json::Value::String(s) => Value::String(s),
        serde_json::Value::Array(items) => Value::List(items.into_iter().map(from_json).collect()),
        serde_json::Value::Object(entries) => Value::Compound(
            entries
                .into_iter()
                .map(|(key, value)| (key, from_json(value)))
                .collect(),
        ),
        _ => Value::Other,
    }
}

/// The component's text without styling: literal text, or the key of a
/// translation, keybind, selector or score, followed by its siblings.
pub fn plain(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::List(items) => items.iter().map(plain).collect(),
        Value::Compound(entries) => {
            let get = |key: &str| entries.iter().find(|(k, _)| k == key).map(|(_, v)| v);
            // A lone "" key wraps a list element of a different tag type.
            if let [(key, inner)] = entries.as_slice()
                && key.is_empty()
            {
                return plain(inner);
            }
            let score = get("score").and_then(|score| match score {
                Value::Compound(fields) => fields.iter().find(|(k, _)| k == "name").map(|(_, v)| v),
                _ => None,
            });
            let own = ["text", "translate", "keybind", "selector"]
                .into_iter()
                .find_map(get)
                .or(score)
                .map(plain)
                .unwrap_or_default();
            let extra = get("extra").map(plain).unwrap_or_default();
            own + &extra
        }
        Value::Other => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::nbt::tests::{compound, named_string, utf};
    use super::*;
    use crate::fields::reader::Reader;
    use crate::fields::testing::lines;

    fn decode(protocol: i32, bytes: &[u8]) -> Vec<String> {
        let mut d = Decoder::new(Reader::new(bytes), protocol);
        component(&mut d, "message").unwrap();
        let (reader, fields) = d.into_parts();
        assert_eq!(reader.remaining(), 0);
        lines(&fields)
    }

    #[test]
    fn json_before_1_20_3() {
        let json = r#"{"text":"Hello ","extra":[{"translate":"chat.type.text"},"!"]}"#;
        let bytes = [&[json.len() as u8][..], json.as_bytes()].concat();
        let end = bytes.len();
        assert_eq!(
            decode(V1_20_3 - 1, &bytes),
            [
                format!("message: \"Hello chat.type.text!\" [0..{end}]"),
                format!("  json: {json:?} [0..{end}]"),
            ]
        );
    }

    #[test]
    fn nbt_from_1_20_3() {
        let extra = [
            &[9][..],
            &utf("extra"),
            &[8],
            &1i32.to_be_bytes(),
            &utf("!"),
        ]
        .concat();
        let bytes = compound(&[named_string("text", "Hi"), extra]);
        assert_eq!(
            decode(V1_20_3, &bytes),
            [
                "message: \"Hi!\" [0..29]",
                "  text: \"Hi\" [8..12]",
                "  extra: 1 item [20..28]",
                "    0: \"!\" [25..28]",
            ]
        );
        assert_eq!(
            decode(V1_20_3, &[&[8][..], &utf("plain")].concat()),
            ["message: \"plain\" [0..8]"]
        );
    }

    #[test]
    fn plain_text_unwraps_and_reads_scores() {
        let wrapped = Value::Compound(vec![(String::new(), Value::String("a".into()))]);
        let score = Value::Compound(vec![(
            "score".into(),
            Value::Compound(vec![("name".into(), Value::String("@p".into()))]),
        )]);
        assert_eq!(plain(&Value::List(vec![wrapped, score])), "a@p");
    }
}
