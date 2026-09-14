use super::{Edit, Error, Format, KeyPath};
use jsonc_parser::{
    CollectOptions, CommentCollectionStrategy, ParseOptions, common::Ranged, cst::CstRootNode,
    parse_to_ast,
};
use std::collections::BTreeSet;
use toml_edit::{DocumentMut, InlineTable, Item, Table, TableLike, Value};

pub(crate) fn edit_value(edit: &Edit) -> Option<serde_json::Value> {
    match edit {
        Edit::Set { value, .. } => value.value(),
        Edit::Remove { .. } => None,
    }
}

pub(crate) fn parse_document(format: Format, bytes: &[u8]) -> Result<serde_json::Value, Error> {
    let source = std::str::from_utf8(bytes)
        .map_err(|_| Error::message("client config parse failed: document is not UTF-8"))?;
    match format {
        Format::StrictJson | Format::JsonWithComments => parse_json(format, source),
        Format::Toml => {
            source
                .parse::<DocumentMut>()
                .map_err(|_| Error::message("client config TOML parse failed"))?;
            toml_edit::de::from_str(source)
                .map_err(|_| Error::message("client config TOML value conversion failed"))
        }
    }
}

fn json_options(format: Format) -> ParseOptions {
    ParseOptions {
        allow_comments: format == Format::JsonWithComments,
        allow_loose_object_property_names: false,
        allow_trailing_commas: false,
        allow_missing_commas: false,
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
    }
}

fn parse_json(format: Format, source: &str) -> Result<serde_json::Value, Error> {
    let root = CstRootNode::parse(source, &json_options(format))
        .map_err(|_| Error::message("client config JSON parse failed"))?;
    let object = root
        .object_value()
        .ok_or_else(|| Error::message("client config JSON root must be an object"))?;
    reject_duplicate_json_keys(&object)?;
    let strict_source = match format {
        Format::StrictJson => source.as_bytes().to_vec(),
        Format::JsonWithComments => mask_json_comments(source)?,
        Format::Toml => unreachable!("TOML is not JSON"),
    };
    let value: serde_json::Value = serde_json::from_slice(&strict_source)
        .map_err(|_| Error::message("client config JSON parse failed strict validation"))?;
    if !value.is_object() {
        return Err(Error::message("client config JSON root must be an object"));
    }
    Ok(value)
}

fn mask_json_comments(source: &str) -> Result<Vec<u8>, Error> {
    let parsed = parse_to_ast(
        source,
        &CollectOptions {
            comments: CommentCollectionStrategy::Separate,
            tokens: false,
        },
        &json_options(Format::JsonWithComments),
    )
    .map_err(|_| Error::message("client config JSON comment scan failed"))?;
    let mut masked = source.as_bytes().to_vec();
    let mut ranges = BTreeSet::new();
    for comments in parsed
        .comments
        .into_iter()
        .flat_map(|comments| comments.into_values())
    {
        for comment in comments.iter() {
            let range = comment.range();
            ranges.insert((range.start, range.end));
        }
    }
    for (start, end) in ranges {
        let Some(slice) = masked.get_mut(start..end) else {
            return Err(Error::message(
                "client config JSON comment range is invalid",
            ));
        };
        for byte in slice {
            if !matches!(*byte, b'\r' | b'\n') {
                *byte = b' ';
            }
        }
    }
    Ok(masked)
}

fn reject_duplicate_json_keys(object: &jsonc_parser::cst::CstObject) -> Result<(), Error> {
    let mut names = BTreeSet::new();
    for property in object.properties() {
        let name = property
            .name()
            .and_then(|name| match name {
                jsonc_parser::cst::ObjectPropName::String(value) => value.decoded_value().ok(),
                jsonc_parser::cst::ObjectPropName::Word(_) => None,
            })
            .ok_or_else(|| Error::message("client config JSON property name is unsupported"))?;
        if !names.insert(name) {
            return Err(Error::message(
                "client config JSON has an ambiguous duplicate key",
            ));
        }
        if let Some(child) = property.object_value() {
            reject_duplicate_json_keys(&child)?;
        }
        if let Some(array) = property.array_value() {
            reject_duplicate_json_array(&array)?;
        }
    }
    Ok(())
}

fn reject_duplicate_json_array(array: &jsonc_parser::cst::CstArray) -> Result<(), Error> {
    for element in array.elements() {
        if let Some(object) = element.as_object() {
            reject_duplicate_json_keys(&object)?;
        }
        if let Some(array) = element.as_array() {
            reject_duplicate_json_array(&array)?;
        }
    }
    Ok(())
}

pub(crate) fn render_edits(
    format: Format,
    source: &[u8],
    edits: &[Edit],
) -> Result<Vec<u8>, Error> {
    match format {
        Format::StrictJson | Format::JsonWithComments => render_json(format, source, edits),
        Format::Toml => render_toml(source, edits),
    }
}

fn render_json(format: Format, source: &[u8], edits: &[Edit]) -> Result<Vec<u8>, Error> {
    let source = std::str::from_utf8(source)
        .map_err(|_| Error::message("client config JSON is not UTF-8"))?;
    let root = CstRootNode::parse(source, &json_options(format))
        .map_err(|_| Error::message("client config JSON parse failed"))?;
    let object = root
        .object_value()
        .ok_or_else(|| Error::message("client config JSON root must be an object"))?;
    reject_duplicate_json_keys(&object)?;
    let semantic = parse_json(format, source)?;
    let mut changed = false;
    for edit in edits {
        let desired = edit_value(edit);
        if get_value(&semantic, edit.key()).cloned() == desired {
            continue;
        }
        changed |= edit_json_object(&object, edit.key().components(), desired.as_ref())?;
    }
    if !changed {
        return Ok(source.as_bytes().to_vec());
    }
    let rendered = root.to_string().into_bytes();
    parse_document(format, &rendered)?;
    Ok(rendered)
}

fn edit_json_object(
    object: &jsonc_parser::cst::CstObject,
    path: &[String],
    value: Option<&serde_json::Value>,
) -> Result<bool, Error> {
    let (head, tail) = path
        .split_first()
        .ok_or_else(|| Error::message("owned key path is empty"))?;
    if tail.is_empty() {
        return match (object.get(head), value) {
            (Some(property), Some(value)) => {
                property.set_value(json_input(value)?);
                Ok(true)
            }
            (None, Some(value)) => {
                object.append(head, json_input(value)?);
                Ok(true)
            }
            (Some(property), None) => {
                property.remove();
                Ok(true)
            }
            (None, None) => Ok(false),
        };
    }
    let child = match object.get(head) {
        Some(property) => property.object_value().ok_or_else(|| {
            Error::message("JSON key path crosses a non-object value; unsafe form refused")
        })?,
        None if value.is_none() => return Ok(false),
        None => object
            .append(head, jsonc_parser::cst::CstInputValue::Object(Vec::new()))
            .object_value()
            .expect("inserted object"),
    };
    edit_json_object(&child, tail, value)
}

fn json_input(value: &serde_json::Value) -> Result<jsonc_parser::cst::CstInputValue, Error> {
    use jsonc_parser::cst::CstInputValue;
    Ok(match value {
        serde_json::Value::Null => CstInputValue::Null,
        serde_json::Value::Bool(value) => CstInputValue::Bool(*value),
        serde_json::Value::Number(value) => CstInputValue::Number(value.to_string()),
        serde_json::Value::String(value) => CstInputValue::String(value.clone()),
        serde_json::Value::Array(values) => CstInputValue::Array(
            values
                .iter()
                .map(json_input)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        serde_json::Value::Object(values) => CstInputValue::Object(
            values
                .iter()
                .map(|(key, value)| Ok((key.clone(), json_input(value)?)))
                .collect::<Result<Vec<_>, Error>>()?,
        ),
    })
}

fn render_toml(source: &[u8], edits: &[Edit]) -> Result<Vec<u8>, Error> {
    let source = std::str::from_utf8(source)
        .map_err(|_| Error::message("client config TOML is not UTF-8"))?;
    let mut document = source
        .parse::<DocumentMut>()
        .map_err(|_| Error::message("client config TOML parse failed"))?;
    let semantic: serde_json::Value = toml_edit::de::from_str(source)
        .map_err(|_| Error::message("client config TOML value conversion failed"))?;
    let mut changed = false;
    for edit in edits {
        if toml_item_at(document.as_table(), edit.key().components())
            .is_some_and(toml_item_contains_unsupported_native_value)
        {
            return Err(Error::message(
                "TOML owned value contains a native date or time or a non-finite float that cannot be safely restored; this edit is unsupported",
            ));
        }
        let desired = edit_value(edit);
        if get_value(&semantic, edit.key()).cloned() == desired {
            continue;
        }
        changed |= edit_toml_table(
            document.as_table_mut(),
            edit.key().components(),
            desired.as_ref(),
        )?;
    }
    if !changed {
        return Ok(source.as_bytes().to_vec());
    }
    let rendered = document.to_string().into_bytes();
    parse_document(Format::Toml, &rendered)?;
    Ok(rendered)
}

fn toml_item_at<'a>(table: &'a dyn TableLike, path: &[String]) -> Option<&'a Item> {
    let (head, tail) = path.split_first()?;
    let item = table.get(head)?;
    if tail.is_empty() {
        Some(item)
    } else {
        toml_item_at(item.as_table_like()?, tail)
    }
}

fn toml_item_contains_unsupported_native_value(item: &Item) -> bool {
    match item {
        Item::None => false,
        Item::Value(value) => toml_value_contains_unsupported_native_value(value),
        Item::Table(table) => table
            .iter()
            .any(|(_, item)| toml_item_contains_unsupported_native_value(item)),
        Item::ArrayOfTables(tables) => tables.iter().any(|table| {
            table
                .iter()
                .any(|(_, item)| toml_item_contains_unsupported_native_value(item))
        }),
    }
}

fn toml_value_contains_unsupported_native_value(value: &Value) -> bool {
    match value {
        Value::Datetime(_) => true,
        Value::Float(value) => !value.value().is_finite(),
        Value::Array(array) => array
            .iter()
            .any(toml_value_contains_unsupported_native_value),
        Value::InlineTable(table) => table
            .iter()
            .any(|(_, value)| toml_value_contains_unsupported_native_value(value)),
        Value::String(_) | Value::Integer(_) | Value::Boolean(_) => false,
    }
}

fn edit_toml_table(
    table: &mut dyn TableLike,
    path: &[String],
    value: Option<&serde_json::Value>,
) -> Result<bool, Error> {
    let (head, tail) = path
        .split_first()
        .ok_or_else(|| Error::message("owned key path is empty"))?;
    if tail.is_empty() {
        return match value {
            Some(value) => {
                let value = toml_value(value)?;
                let item = table.entry(head).or_insert(Item::None);
                let decor = item.as_value().map(|old| old.decor().clone());
                *item = Item::Value(value);
                if let (Some(decor), Some(value)) = (decor, item.as_value_mut()) {
                    *value.decor_mut() = decor;
                }
                Ok(true)
            }
            None => Ok(table.remove(head).is_some()),
        };
    }
    if !table.contains_key(head) {
        if value.is_none() {
            return Ok(false);
        }
        table.insert(head, Item::Table(Table::new()));
    }
    let child = table
        .get_mut(head)
        .and_then(Item::as_table_like_mut)
        .ok_or_else(|| {
            Error::message("TOML key path crosses a non-table value; unsafe form refused")
        })?;
    edit_toml_table(child, tail, value)
}

fn toml_value(value: &serde_json::Value) -> Result<Value, Error> {
    Ok(match value {
        serde_json::Value::Null => {
            return Err(Error::message("TOML has no safe null value representation"));
        }
        serde_json::Value::Bool(value) => Value::from(*value),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Value::from(value)
            } else if let Some(value) = value.as_u64() {
                let value = i64::try_from(value)
                    .map_err(|_| Error::message("TOML integer is outside the supported range"))?;
                Value::from(value)
            } else {
                Value::from(
                    value
                        .as_f64()
                        .ok_or_else(|| Error::message("TOML number is unsupported"))?,
                )
            }
        }
        serde_json::Value::String(value) => Value::from(value.as_str()),
        serde_json::Value::Array(values) => {
            let mut array = toml_edit::Array::new();
            for value in values {
                array.push(toml_value(value)?);
            }
            Value::Array(array)
        }
        serde_json::Value::Object(values) => {
            let mut table = InlineTable::new();
            for (key, value) in values {
                table.insert(key, toml_value(value)?);
            }
            Value::InlineTable(table)
        }
    })
}

pub(crate) fn get_value<'a>(
    document: &'a serde_json::Value,
    key: &KeyPath,
) -> Option<&'a serde_json::Value> {
    let mut current = document;
    for component in key.components() {
        current = current.as_object()?.get(component)?;
    }
    Some(current)
}
