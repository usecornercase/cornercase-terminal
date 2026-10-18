use serde_json::Value;

use crate::control::contract::{self, CONTRACT, Code, Descriptor, Refusal};
use crate::control::{self, Command, Request};
use crate::version;

const DEVELOPMENT: &str = "this is a development build, which never looks for updates";

pub(super) const NOT_LOOKING: &[(&str, &str)] = &[("update-check", DEVELOPMENT), ("update-install", DEVELOPMENT)];

const OPT_IN: [&str; 2] = ["contract", "boot"];

const NULLABLE: [&str; 3] = ["contract", "boot", "view"];

pub(super) struct Parsed {
    pub(super) structured: bool,
    pub(super) request: Result<Request, Refusal>,
}

fn opted_in(value: &Value) -> bool {
    OPT_IN.iter().any(|key| value.get(key).is_some_and(|v| !v.is_null()))
}

pub(super) fn parse(text: &str, unavailable: &[(&str, &str)]) -> Parsed {
    let value: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(e) => {
            let refusal = Refusal::new(Code::Malformed, format!("the request is not JSON: {e}"));
            return Parsed { structured: false, request: Err(refusal) };
        }
    };
    let structured = opted_in(&value);
    Parsed { structured, request: read(&value, structured, unavailable) }
}

fn without_null_opt_in(value: &Value) -> Value {
    let mut value = value.clone();
    if let Value::Object(map) = &mut value {
        map.retain(|key, inner| !(inner.is_null() && NULLABLE.contains(&key.as_str())));
    }
    value
}

pub(super) fn read(value: &Value, structured: bool, unavailable: &[(&str, &str)]) -> Result<Request, Refusal> {
    let name = control::command_name(value).unwrap_or_default().to_string();
    if !Command::NAMES.contains(&name.as_str()) {
        return Err(Refusal::new(Code::Unsupported, control::unknown_command(&name)));
    }
    if let Some((_, why)) = unavailable.iter().find(|(off, _)| *off == name) {
        return Err(Refusal::new(Code::Unavailable, format!("`{name}` is not available on this server: {why}")));
    }
    if let Some(asked) = value.get("contract").and_then(Value::as_u64).filter(|asked| *asked > u64::from(CONTRACT)) {
        return Err(Refusal::new(
            Code::Unsupported,
            format!("this server speaks contract {CONTRACT}, not {asked}; update cornercase and restart the server"),
        ));
    }
    let request: Request = serde_json::from_value(value.clone()).map_err(|e| {
        let server = version::CURRENT;
        Refusal::new(
            Code::Malformed,
            format!("the running cornercase server ({server}) cannot read this `{name}` request: {e}"),
        )
    })?;
    match contract::unknown_fields(&without_null_opt_in(value), &request).filter(|_| structured) {
        Some(path) => {
            Err(Refusal::new(Code::UnknownField, format!("this `{name}` request has an unknown field `{path}`")))
        }
        None => Ok(request),
    }
}

pub(super) fn descriptor(unavailable: &[(&str, &str)]) -> Descriptor {
    contract::describe(control::server_token(), unavailable)
}
