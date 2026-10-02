use super::{JavaScriptExtractor, TypeScriptExtractor, source};
use syntaxmesh_core::ImportKind;
use syntaxmesh_language_sdk::{Extraction, ExtractorError, LanguageExtractor};

fn extract(path: &str, content: &str) -> Result<Extraction, ExtractorError> {
    let input = source(path, content);
    if path.ends_with(".ts") {
        TypeScriptExtractor.extract(&input)
    } else {
        JavaScriptExtractor.extract(&input)
    }
}

#[test]
fn literal_members_decode_names_and_preserve_quoted_evidence() -> Result<(), ExtractorError> {
    let content = r#"import * as schema from './schema';
const first = schema['user'];
const second = schema?.["optionalMember"];
const escaped = schema["\u0075ser"];
const unicode = schema['café'];
"#;
    for path in ["src/app.ts", "src/app.js"] {
        let extraction = extract(path, content)?;
        let members = extraction
            .imports
            .iter()
            .filter(|record| record.kind == ImportKind::NamespaceMember)
            .collect::<Vec<_>>();
        let expected = [
            ("user", "'user'"),
            ("optionalMember", "\"optionalMember\""),
            ("user", r#""\u0075ser""#),
            ("café", "'café'"),
        ];
        if members.len() != expected.len() {
            return Err(ExtractorError::InvalidInput(
                "literal member count mismatch".to_owned(),
            ));
        }
        for (record, (name, quoted)) in members.iter().zip(expected) {
            let start = usize::try_from(record.source.span.start_byte)
                .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
            let end = usize::try_from(record.source.span.end_byte)
                .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
            if record.specifier != "./schema"
                || record.imported_name.as_deref() != Some(name)
                || record.local_name.as_deref() != Some(format!("schema.{name}").as_str())
                || content.get(start..end) != Some(quoted)
                || record.type_only
            {
                return Err(ExtractorError::InvalidInput(
                    "literal member evidence mismatch".to_owned(),
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn dynamic_keys_and_indirect_namespace_objects_remain_excluded() -> Result<(), ExtractorError> {
    let content = r#"import * as schema from './schema';
const key = 'user';
const alias = schema;
const values = [schema[key], schema['us' + 'er'], schema[`user`],
    schema[1], alias['user'], schema.nested['user'], schema?.[key]];
"#;
    for path in ["src/dynamic.ts", "src/dynamic.js"] {
        let extraction = extract(path, content)?;
        // The static schema.nested access is still recorded; its subsequent key is not.
        let members = extraction
            .imports
            .iter()
            .filter(|record| record.kind == ImportKind::NamespaceMember)
            .collect::<Vec<_>>();
        if members.len() != 1
            || !members
                .iter()
                .all(|record| record.imported_name.as_deref() == Some("nested"))
        {
            return Err(ExtractorError::InvalidInput(
                "dynamic or indirect key was evaluated".to_owned(),
            ));
        }
    }
    Ok(())
}

#[test]
fn literal_members_fail_closed_for_shadowed_namespace_locals() -> Result<(), ExtractorError> {
    for body in [
        "function load(schema) { return schema['secret']; }",
        "const load = (schema) => schema?.['secret'];",
        "try {} catch (schema) { schema['secret']; }",
        "schema['user']; { const schema = {}; schema['secret']; }",
    ] {
        for path in ["src/shadowed.ts", "src/shadowed.js"] {
            let extraction = extract(path, &format!("import * as schema from './schema'; {body}"))?;
            if extraction
                .imports
                .iter()
                .any(|record| record.kind == ImportKind::NamespaceMember)
            {
                return Err(ExtractorError::InvalidInput(
                    "shadowed literal member was recorded".to_owned(),
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn repeated_literal_occurrences_have_stable_distinct_identities() -> Result<(), ExtractorError> {
    let content = "import * as schema from './schema'; schema['user']; schema['user'];";
    for path in ["src/identity.ts", "src/identity.js"] {
        let first = extract(path, content)?;
        let moved = extract(path, &format!("\n\n{content}"))?;
        let ids = |extraction: &Extraction| {
            extraction
                .imports
                .iter()
                .filter(|record| record.kind == ImportKind::NamespaceMember)
                .map(|record| record.id)
                .collect::<Vec<_>>()
        };
        let first_ids = ids(&first);
        let distinct = first_ids.iter().collect::<std::collections::BTreeSet<_>>();
        if first_ids.len() != 2 || distinct.len() != 2 || first_ids != ids(&moved) {
            return Err(ExtractorError::InvalidInput(
                "literal member identities are unstable".to_owned(),
            ));
        }
    }
    Ok(())
}
