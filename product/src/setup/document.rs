//! Comment-preserving edits of an already validated setup document.
use super::Error;
use crate::config::{Accounting, Auth, CancelPolicy, Config, Limit};
use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, TableLike, Value};

pub(super) fn render_preserving(
    bytes: &[u8],
    original: &Config,
    config: &Config,
) -> Result<String, Error> {
    let source = std::str::from_utf8(bytes)
        .map_err(|_| Error::message("existing configuration is not UTF-8"))?;
    let mut document = source
        .parse::<DocumentMut>()
        .map_err(|_| Error::message("existing configuration document cannot be edited"))?;

    if original.listen != config.listen {
        replace(
            &mut document["listen"],
            Value::from(config.listen.to_string()),
        );
    }
    if original.concurrency != config.concurrency {
        replace(
            &mut document["concurrency"],
            Value::from(i64::from(config.concurrency)),
        );
    }
    if original.cancel_policy != config.cancel_policy {
        replace(
            &mut document["cancel_policy"],
            Value::from(match config.cancel_policy {
                CancelPolicy::Drain => "drain",
                CancelPolicy::Close => "close",
            }),
        );
    }
    if original.accounting != config.accounting {
        replace(
            &mut document["accounting"],
            Value::from(match config.accounting {
                Accounting::Reserved => "reserved",
                Accounting::Actual => "actual",
            }),
        );
    }
    if original.retry_transient_429 != config.retry_transient_429 {
        replace(
            &mut document["retry_transient_429"],
            Value::from(config.retry_transient_429),
        );
    }

    if original.upstream != config.upstream {
        edit_upstream(&mut document, original, config)?;
    }
    if original.quota != config.quota {
        edit_quota(&mut document, original, config)?;
    }
    if original.models != config.models {
        edit_models(&mut document["models"], original, config)?;
    }
    if original.roots != config.roots {
        edit_roots(&mut document["roots"], original, config)?;
    }

    let rendered = document.to_string();
    let parsed = crate::config::parse(rendered.as_bytes())?;
    if parsed != *config {
        return Err(Error::message(
            "edited configuration does not match the reviewed setup choices",
        ));
    }
    Ok(rendered)
}

fn edit_upstream(
    document: &mut DocumentMut,
    original: &Config,
    config: &Config,
) -> Result<(), Error> {
    let upstream = table_like(&mut document["upstream"], "upstream")?;
    if original.upstream.api_base != config.upstream.api_base {
        replace_field(
            upstream,
            "api_base",
            Value::from(config.upstream.api_base.as_str()),
        );
    }
    if original.upstream.proxy != config.upstream.proxy {
        optional_field(
            upstream,
            "proxy",
            config
                .upstream
                .proxy
                .as_ref()
                .map(|proxy| Value::from(proxy.as_str())),
        );
    }
    if original.upstream.ca_bundle != config.upstream.ca_bundle {
        optional_field(
            upstream,
            "ca_bundle",
            config
                .upstream
                .ca_bundle
                .as_ref()
                .map(|path| Value::from(path.to_string_lossy().as_ref())),
        );
    }

    let (mode, header, name) = match &config.upstream.auth {
        Auth::Forward => ("forward", None, None),
        Auth::None => ("none", None, None),
        Auth::Env { header, name } => ("env", Some(header.as_str()), Some(name.as_str())),
    };
    if original.upstream.auth != config.upstream.auth {
        let auth_item = upstream
            .get_mut("auth")
            .ok_or_else(|| Error::message("validated upstream auth is missing"))?;
        let auth = table_like(auth_item, "upstream auth")?;
        replace_field(auth, "mode", Value::from(mode));
        optional_field(auth, "header", header.map(Value::from));
        optional_field(auth, "name", name.map(Value::from));
    }
    Ok(())
}

fn edit_quota(document: &mut DocumentMut, original: &Config, config: &Config) -> Result<(), Error> {
    let quota = table_like(&mut document["quota"], "quota")?;
    for (field, old_limit, limit) in [
        ("rpm", &original.quota.rpm, &config.quota.rpm),
        ("tpm", &original.quota.tpm, &config.quota.tpm),
    ] {
        if old_limit == limit {
            continue;
        }
        let (kind, value) = match limit {
            Limit::Known(value) => ("known", Some(value.get())),
            Limit::Unknown => ("unknown", None),
            Limit::Unlimited => ("unlimited", None),
        };
        let limit_item = quota
            .get_mut(field)
            .ok_or_else(|| Error::message(format!("validated quota {field} is missing")))?;
        let table = table_like(limit_item, "quota limit")?;
        replace_field(table, "kind", Value::from(kind));
        optional_field(table, "value", value.map(|value| Value::from(value as i64)));
    }
    Ok(())
}

fn edit_models(item: &mut Item, original: &Config, config: &Config) -> Result<(), Error> {
    edit_table_sequence(item, "models", config.models.len(), |index, table| {
        let model = &config.models[index];
        let old = original.models.get(index);
        if old.is_none_or(|old| old.id != model.id) {
            replace_field(table, "id", Value::from(model.id.as_str()));
        }
        if old.is_none_or(|old| old.max_output_tokens != model.max_output_tokens) {
            optional_field(
                table,
                "max_output_tokens",
                model
                    .max_output_tokens
                    .map(|value| Value::from(value.get() as i64)),
            );
        }
    })
}

fn edit_roots(item: &mut Item, original: &Config, config: &Config) -> Result<(), Error> {
    edit_table_sequence(item, "roots", config.roots.len(), |index, table| {
        let root = &config.roots[index];
        let old = original.roots.get(index);
        if old.is_none_or(|old| old.id != root.id) {
            replace_field(table, "id", Value::from(root.id.as_str()));
        }
        if old.is_none_or(|old| old.endpoints != root.endpoints) {
            replace_field(
                table,
                "endpoints",
                string_array(root.endpoints.iter().map(|endpoint| endpoint.path())),
            );
        }
        if old.is_none_or(|old| old.models != root.models) {
            replace_field(
                table,
                "models",
                string_array(root.models.iter().map(String::as_str)),
            );
        }
    })
}

fn edit_table_sequence(
    item: &mut Item,
    label: &str,
    desired_len: usize,
    mut edit: impl FnMut(usize, &mut dyn TableLike),
) -> Result<(), Error> {
    if let Some(tables) = item.as_array_of_tables_mut() {
        while tables.len() > desired_len {
            tables.remove(tables.len() - 1);
        }
        while tables.len() < desired_len {
            tables.push(Table::new());
        }
        for index in 0..desired_len {
            let table = tables
                .get_mut(index)
                .ok_or_else(|| Error::message(format!("validated {label} table is missing")))?;
            edit(index, table);
        }
        return Ok(());
    }

    let array = item
        .as_value_mut()
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            Error::message(format!("validated {label} is not an editable table list"))
        })?;
    while array.len() > desired_len {
        array.remove(array.len() - 1);
    }
    while array.len() < desired_len {
        array.push(InlineTable::new());
    }
    for index in 0..desired_len {
        let table = array
            .get_mut(index)
            .and_then(Value::as_inline_table_mut)
            .ok_or_else(|| Error::message(format!("validated {label} entry is not table-like")))?;
        edit(index, table);
    }
    Ok(())
}

fn string_array<'a>(values: impl IntoIterator<Item = &'a str>) -> Value {
    let mut array = Array::new();
    for value in values {
        array.push(value);
    }
    Value::Array(array)
}

fn replace(item: &mut Item, value: Value) {
    let decor = item.as_value().map(|old| old.decor().clone());
    *item = Item::Value(value);
    if let (Some(decor), Some(value)) = (decor, item.as_value_mut()) {
        *value.decor_mut() = decor;
    }
}

fn replace_field(table: &mut dyn TableLike, key: &str, value: Value) {
    replace(table.entry(key).or_insert(Item::None), value);
}

fn optional_field(table: &mut dyn TableLike, key: &str, value: Option<Value>) {
    match value {
        Some(value) => replace_field(table, key, value),
        None => {
            table.remove(key);
        }
    }
}

fn table_like<'a>(item: &'a mut Item, label: &str) -> Result<&'a mut dyn TableLike, Error> {
    item.as_table_like_mut()
        .ok_or_else(|| Error::message(format!("validated {label} is not table-like")))
}
