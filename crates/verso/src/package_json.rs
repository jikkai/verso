use noyalib::borrowed::{from_str_borrowed, BorrowedValue};
use semver::Version;
use serde_json::Value as JsonValue;
use std::fs;
use std::ops::Range;
use std::path::Path;

const PACKAGE_MANIFEST_NAMES: [&str; 4] = [
    "package.json",
    "package.json5",
    "package.yaml",
    "package.yml",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageInfo {
    pub name: Option<String>,
    pub version: Version,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageManifest {
    pub info: PackageInfo,
    pub workspaces: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManifestFormat {
    Json,
    Json5,
    Yaml,
}

pub fn manifest_names() -> &'static [&'static str] {
    &PACKAGE_MANIFEST_NAMES
}

pub fn read_package(path: &Path) -> Result<PackageInfo, String> {
    read_package_manifest(path).map(|manifest| manifest.info)
}

pub fn read_package_manifest(path: &Path) -> Result<PackageManifest, String> {
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;

    read_package_manifest_from_str(path, &contents)
}

pub fn read_package_from_str(path: &Path, contents: &str) -> Result<PackageInfo, String> {
    read_package_manifest_from_str(path, contents).map(|manifest| manifest.info)
}

pub fn read_package_manifest_from_str(
    path: &Path,
    contents: &str,
) -> Result<PackageManifest, String> {
    match manifest_format(path) {
        ManifestFormat::Json => {
            let value: JsonValue = serde_json::from_str(contents)
                .map_err(|error| format!("failed to parse {} as JSON: {error}", path.display()))?;
            package_manifest_from_json_value(path, &value)
        }
        ManifestFormat::Json5 => {
            let value: JsonValue = json5::from_str(contents)
                .map_err(|error| format!("failed to parse {} as JSON5: {error}", path.display()))?;
            package_manifest_from_json_value(path, &value)
        }
        ManifestFormat::Yaml => {
            let value = from_str_borrowed(contents)
                .map_err(|error| format!("failed to parse {} as YAML: {error}", path.display()))?;
            package_manifest_from_yaml_value(path, &value)
        }
    }
}

pub fn replace_version_preserving_style(contents: &str, next: &Version) -> Result<String, String> {
    replace_manifest_version_preserving_style(Path::new("package.json"), contents, next)
}

pub fn replace_manifest_version_preserving_style(
    path: &Path,
    contents: &str,
    next: &Version,
) -> Result<String, String> {
    read_package_manifest_from_str(path, contents)?;

    let range = match manifest_format(path) {
        ManifestFormat::Json | ManifestFormat::Json5 => {
            find_top_level_json_version_value_range(contents)?
        }
        ManifestFormat::Yaml => find_top_level_yaml_version_value_range(contents)?,
    }
    .ok_or_else(|| "failed to locate top-level package version string".to_owned())?;

    let mut updated = String::with_capacity(contents.len() + next.to_string().len());
    updated.push_str(&contents[..range.start]);
    updated.push_str(&next.to_string());
    updated.push_str(&contents[range.end..]);
    Ok(updated)
}

fn manifest_format(path: &Path) -> ManifestFormat {
    match path.file_name().and_then(|name| name.to_str()) {
        Some("package.json5") => ManifestFormat::Json5,
        Some("package.yaml" | "package.yml") => ManifestFormat::Yaml,
        _ => ManifestFormat::Json,
    }
}

fn package_manifest_from_json_value(
    path: &Path,
    value: &JsonValue,
) -> Result<PackageManifest, String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{} must contain a package manifest object", path.display()))?;
    let name = object
        .get("name")
        .and_then(JsonValue::as_str)
        .map(ToOwned::to_owned);
    let context = package_context(path, name.as_deref());
    let version_value = object
        .get("version")
        .ok_or_else(|| missing_version_error(&context))?;
    let version_string = version_value
        .as_str()
        .ok_or_else(|| format!("{context} field \"version\" must be a string"))?;
    let version = Version::parse(version_string).map_err(|error| {
        format!("{context} has invalid semver version \"{version_string}\": {error}")
    })?;
    let workspaces = object
        .get("workspaces")
        .and_then(workspaces_from_json_value);

    Ok(PackageManifest {
        info: PackageInfo { name, version },
        workspaces,
    })
}

fn package_manifest_from_yaml_value(
    path: &Path,
    value: &BorrowedValue<'_>,
) -> Result<PackageManifest, String> {
    let object = value
        .as_mapping()
        .ok_or_else(|| format!("{} must contain a package manifest object", path.display()))?;
    let name = object
        .get("name")
        .and_then(BorrowedValue::as_str)
        .map(ToOwned::to_owned);
    let context = package_context(path, name.as_deref());
    let version_value = object
        .get("version")
        .ok_or_else(|| missing_version_error(&context))?;
    let version_string = version_value
        .as_str()
        .ok_or_else(|| format!("{context} field \"version\" must be a string"))?;
    let version = Version::parse(version_string).map_err(|error| {
        format!("{context} has invalid semver version \"{version_string}\": {error}")
    })?;
    let workspaces = object
        .get("workspaces")
        .and_then(workspaces_from_yaml_value);

    Ok(PackageManifest {
        info: PackageInfo { name, version },
        workspaces,
    })
}

fn workspaces_from_json_value(value: &JsonValue) -> Option<Vec<String>> {
    if let Some(items) = value.as_array() {
        return string_array_from_json(items);
    }
    value
        .as_object()
        .and_then(|object| object.get("packages"))
        .and_then(JsonValue::as_array)
        .and_then(|items| string_array_from_json(items))
}

fn string_array_from_json(items: &[JsonValue]) -> Option<Vec<String>> {
    items
        .iter()
        .map(|item| item.as_str().map(ToOwned::to_owned))
        .collect()
}

fn workspaces_from_yaml_value(value: &BorrowedValue<'_>) -> Option<Vec<String>> {
    if let Some(items) = value.as_sequence() {
        return string_array_from_yaml(items);
    }
    value
        .as_mapping()
        .and_then(|object| object.get("packages"))
        .and_then(BorrowedValue::as_sequence)
        .and_then(|items| string_array_from_yaml(items))
}

fn string_array_from_yaml(items: &[BorrowedValue<'_>]) -> Option<Vec<String>> {
    items
        .iter()
        .map(|item| item.as_str().map(ToOwned::to_owned))
        .collect()
}

fn package_context(path: &Path, name: Option<&str>) -> String {
    match name {
        Some(name) => format!("package {name} ({})", path.display()),
        None => format!("package {}", path.display()),
    }
}

fn missing_version_error(context: &str) -> String {
    format!(
        "{context} is missing required string field \"version\"\n\nhelp: add a valid SemVer string to \"version\", or exclude this package with workspaces.ignore in verso.toml"
    )
}

fn find_top_level_json_version_value_range(contents: &str) -> Result<Option<Range<usize>>, String> {
    let bytes = contents.as_bytes();
    let mut cursor = skip_json_whitespace_and_comments(bytes, 0);
    let mut version_range = None;

    if bytes.get(cursor) != Some(&b'{') {
        return Ok(None);
    }
    cursor += 1;

    loop {
        cursor = skip_json_whitespace_and_comments(bytes, cursor);
        match bytes.get(cursor) {
            Some(b'}') => return Ok(version_range),
            Some(b'"' | b'\'') | Some(b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'$') => {}
            _ => return Ok(None),
        }

        let Some((key, key_end)) = parse_json_key(contents, bytes, cursor) else {
            return Ok(None);
        };
        cursor = skip_json_whitespace_and_comments(bytes, key_end);
        if bytes.get(cursor) != Some(&b':') {
            return Ok(None);
        }
        cursor = skip_json_whitespace_and_comments(bytes, cursor + 1);

        if key == "version" {
            if !matches!(bytes.get(cursor), Some(b'"' | b'\'')) {
                return Ok(None);
            }
            let Some(value_end) = find_json_string_end(bytes, cursor) else {
                return Ok(None);
            };
            if version_range.is_some() {
                return Err("duplicate top-level version keys are not supported".to_owned());
            }
            version_range = Some((cursor + 1)..value_end);
            cursor = value_end + 1;
        } else {
            let Some(next_cursor) = skip_json_value(bytes, cursor) else {
                return Ok(None);
            };
            cursor = next_cursor;
        }

        cursor = skip_json_whitespace_and_comments(bytes, cursor);
        match bytes.get(cursor) {
            Some(b',') => cursor += 1,
            Some(b'}') => return Ok(version_range),
            _ => return Ok(None),
        }
    }
}

fn parse_json_key(contents: &str, bytes: &[u8], cursor: usize) -> Option<(String, usize)> {
    match bytes.get(cursor)? {
        b'"' => {
            let key_end = find_json_string_end(bytes, cursor)?;
            let key = serde_json::from_str::<String>(&contents[cursor..=key_end]).ok()?;
            Some((key, key_end + 1))
        }
        b'\'' => {
            let key_end = find_json_string_end(bytes, cursor)?;
            let key = json5::from_str::<String>(&contents[cursor..=key_end]).ok()?;
            Some((key, key_end + 1))
        }
        b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'$' => {
            let mut end = cursor + 1;
            while matches!(
                bytes.get(end),
                Some(b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'$' | b'-')
            ) {
                end += 1;
            }
            Some((contents[cursor..end].to_owned(), end))
        }
        _ => None,
    }
}

fn skip_json_whitespace_and_comments(bytes: &[u8], mut cursor: usize) -> usize {
    loop {
        while matches!(bytes.get(cursor), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            cursor += 1;
        }
        if bytes.get(cursor) == Some(&b'/') && bytes.get(cursor + 1) == Some(&b'/') {
            cursor += 2;
            while !matches!(bytes.get(cursor), None | Some(b'\n' | b'\r')) {
                cursor += 1;
            }
            continue;
        }
        if bytes.get(cursor) == Some(&b'/') && bytes.get(cursor + 1) == Some(&b'*') {
            cursor += 2;
            while bytes.get(cursor).is_some() {
                if bytes.get(cursor) == Some(&b'*') && bytes.get(cursor + 1) == Some(&b'/') {
                    cursor += 2;
                    break;
                }
                cursor += 1;
            }
            continue;
        }
        return cursor;
    }
}

fn find_json_string_end(bytes: &[u8], start: usize) -> Option<usize> {
    let quote = *bytes.get(start)?;
    if !matches!(quote, b'"' | b'\'') {
        return None;
    }

    let mut cursor = start + 1;
    while let Some(byte) = bytes.get(cursor) {
        match byte {
            b'\\' => cursor += 2,
            value if *value == quote => return Some(cursor),
            _ => cursor += 1,
        }
    }
    None
}

fn skip_json_value(bytes: &[u8], start: usize) -> Option<usize> {
    match bytes.get(start)? {
        b'"' | b'\'' => find_json_string_end(bytes, start).map(|end| end + 1),
        b'{' | b'[' => skip_json_container(bytes, start),
        _ => skip_json_scalar(bytes, start),
    }
}

fn skip_json_container(bytes: &[u8], start: usize) -> Option<usize> {
    let mut stack = vec![matching_close(*bytes.get(start)?)?];
    let mut cursor = start + 1;
    while let Some(byte) = bytes.get(cursor) {
        match byte {
            b'"' | b'\'' => cursor = find_json_string_end(bytes, cursor)? + 1,
            b'{' | b'[' => {
                stack.push(matching_close(*byte)?);
                cursor += 1;
            }
            b'}' | b']' => {
                if stack.pop() != Some(*byte) {
                    return None;
                }
                cursor += 1;
                if stack.is_empty() {
                    return Some(cursor);
                }
            }
            b'/' if bytes.get(cursor + 1) == Some(&b'/') => {
                cursor += 2;
                while !matches!(bytes.get(cursor), None | Some(b'\n' | b'\r')) {
                    cursor += 1;
                }
            }
            b'/' if bytes.get(cursor + 1) == Some(&b'*') => {
                cursor += 2;
                while bytes.get(cursor).is_some() {
                    if bytes.get(cursor) == Some(&b'*') && bytes.get(cursor + 1) == Some(&b'/') {
                        cursor += 2;
                        break;
                    }
                    cursor += 1;
                }
            }
            _ => cursor += 1,
        }
    }

    None
}

fn matching_close(open: u8) -> Option<u8> {
    match open {
        b'{' => Some(b'}'),
        b'[' => Some(b']'),
        _ => None,
    }
}

fn skip_json_scalar(bytes: &[u8], start: usize) -> Option<usize> {
    let mut cursor = start;
    while let Some(byte) = bytes.get(cursor) {
        match byte {
            b',' | b'}' | b']' | b' ' | b'\n' | b'\r' | b'\t' => break,
            _ => cursor += 1,
        }
    }

    (cursor > start).then_some(cursor)
}

fn find_top_level_yaml_version_value_range(contents: &str) -> Result<Option<Range<usize>>, String> {
    let mut offset = 0;
    let mut version_range = None;

    for line in contents.split_inclusive('\n') {
        let line_without_newline = line.trim_end_matches(['\r', '\n']);
        if let Some(range) = yaml_version_value_range_in_line(line_without_newline, offset) {
            if version_range.is_some() {
                return Err("duplicate top-level version keys are not supported".to_owned());
            }
            version_range = Some(range);
        }
        offset += line.len();
    }

    Ok(version_range)
}

fn yaml_version_value_range_in_line(line: &str, line_offset: usize) -> Option<Range<usize>> {
    if line.chars().next().is_some_and(char::is_whitespace) || line.trim_start().starts_with('#') {
        return None;
    }
    let colon = line.find(':')?;
    let key = line[..colon].trim();
    if !matches!(key, "version" | "\"version\"" | "'version'") {
        return None;
    }
    let value_start =
        colon + 1 + line[colon + 1..].find(|character: char| !character.is_whitespace())?;
    let value = &line[value_start..];
    if value.is_empty() || value.starts_with('#') {
        return None;
    }
    let (start_delta, end_delta) = if value.starts_with('"') || value.starts_with('\'') {
        let quote = value.as_bytes()[0] as char;
        let end = find_yaml_quoted_scalar_end(value, quote)?;
        (1, end)
    } else {
        let end = value.find(" #").unwrap_or(value.len());
        (0, value[..end].trim_end().len())
    };

    Some((line_offset + value_start + start_delta)..(line_offset + value_start + end_delta))
}

fn find_yaml_quoted_scalar_end(value: &str, quote: char) -> Option<usize> {
    let bytes = value.as_bytes();
    let mut cursor = 1;
    while let Some(byte) = bytes.get(cursor) {
        match (*byte as char, quote) {
            ('\\', '"') => cursor += 2,
            (character, _) if character == quote => return Some(cursor),
            _ => cursor += 1,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn reads_package_version_and_optional_name() -> Result<(), String> {
        let path = Path::new("packages/widget/package.json");
        let package = read_package_from_str(
            path,
            r#"{
  "name": "@scope/widget",
  "version": "1.2.3-beta.4"
}"#,
        )?;

        assert_eq!(package.name, Some("@scope/widget".to_owned()));
        assert_eq!(
            package.version,
            Version::parse("1.2.3-beta.4").expect("test semver should parse")
        );
        Ok(())
    }

    #[test]
    fn reads_package_without_name() -> Result<(), String> {
        let package = read_package_from_str(
            Path::new("package.json"),
            r#"{
  "version": "0.1.0"
}"#,
        )?;

        assert_eq!(package.name, None);
        assert_eq!(package.version, Version::new(0, 1, 0));
        Ok(())
    }

    #[test]
    fn reads_json5_package_manifest() -> Result<(), String> {
        let package = read_package_from_str(
            Path::new("package.json5"),
            r#"{
  // pnpm accepts JSON5 manifests.
  name: "demo",
  version: "1.2.3",
}"#,
        )?;

        assert_eq!(package.name, Some("demo".to_owned()));
        assert_eq!(package.version, Version::new(1, 2, 3));
        Ok(())
    }

    #[test]
    fn reads_yaml_package_manifest() -> Result<(), String> {
        let package =
            read_package_from_str(Path::new("package.yaml"), "name: demo\nversion: 1.2.3\n")?;

        assert_eq!(package.name, Some("demo".to_owned()));
        assert_eq!(package.version, Version::new(1, 2, 3));
        Ok(())
    }

    #[test]
    fn preserves_formatting_around_version_line() -> Result<(), String> {
        let contents =
            "{\n    \"name\": \"demo\",\n    \"version\": \"1.0.0\",\n    \"private\": true\n}\n";
        let next = Version::new(1, 1, 0);

        let updated = replace_version_preserving_style(contents, &next)?;

        assert_eq!(
            updated,
            "{\n    \"name\": \"demo\",\n    \"version\": \"1.1.0\",\n    \"private\": true\n}\n"
        );
        Ok(())
    }

    #[test]
    fn updates_compact_json() -> Result<(), String> {
        let updated = replace_version_preserving_style(
            r#"{"name":"demo","version":"1.0.0"}"#,
            &Version::new(1, 2, 3),
        )?;

        assert_eq!(updated, r#"{"name":"demo","version":"1.2.3"}"#);
        Ok(())
    }

    #[test]
    fn updates_reordered_top_level_fields() -> Result<(), String> {
        let contents =
            "{\n  \"private\": true,\n  \"name\": \"demo\",\n  \"version\": \"1.0.0\"\n}";

        let updated = replace_version_preserving_style(contents, &Version::new(1, 2, 3))?;

        assert_eq!(
            updated,
            "{\n  \"private\": true,\n  \"name\": \"demo\",\n  \"version\": \"1.2.3\"\n}"
        );
        Ok(())
    }

    #[test]
    fn rejects_duplicate_top_level_version_keys() {
        let error = replace_version_preserving_style(
            r#"{"version":"1.0.0","version":"2.0.0"}"#,
            &Version::new(3, 0, 0),
        )
        .expect_err("duplicate top-level version keys should be rejected");

        assert!(error.contains("duplicate top-level version"));
    }

    #[test]
    fn rejects_missing_version() {
        let error = read_package_from_str(Path::new("package.json"), r#"{"name":"demo"}"#)
            .expect_err("missing version should be rejected");

        assert!(error.contains("package.json"));
        assert!(error.contains("version"));
    }

    #[test]
    fn rejects_invalid_json() {
        let error =
            read_package_from_str(Path::new("broken/package.json"), r#"{"version":"1.0.0""#)
                .expect_err("invalid JSON should be rejected");

        assert!(error.contains("broken/package.json"));
        assert!(error.contains("JSON"));
    }

    #[test]
    fn rejects_non_string_version() {
        let error =
            read_package_from_str(Path::new("packages/demo/package.json"), r#"{"version":1}"#)
                .expect_err("non-string version should be rejected");

        assert!(error.contains("packages/demo/package.json"));
        assert!(error.contains("version"));
        assert!(error.contains("string"));
    }

    #[test]
    fn rejects_invalid_semver() {
        let error = read_package_from_str(
            Path::new("package.json"),
            r#"{"name":"demo","version":"1.2"}"#,
        )
        .expect_err("invalid semver should be rejected");

        assert!(error.contains("demo"));
        assert!(error.contains("1.2"));
        assert!(error.contains("semver"));
    }

    #[test]
    fn preserves_crlf_and_final_newline_style() -> Result<(), String> {
        let contents = "{\r\n  \"version\": \"1.0.0\"\r\n}\r\n";

        let updated = replace_version_preserving_style(contents, &Version::new(2, 0, 0))?;

        assert_eq!(updated, "{\r\n  \"version\": \"2.0.0\"\r\n}\r\n");
        Ok(())
    }

    #[test]
    fn updates_json5_version_without_removing_comments() -> Result<(), String> {
        let contents = "{\n  // release version\n  version: \"1.0.0\",\n}\n";

        let updated = replace_manifest_version_preserving_style(
            Path::new("package.json5"),
            contents,
            &Version::new(1, 1, 0),
        )?;

        assert_eq!(
            updated,
            "{\n  // release version\n  version: \"1.1.0\",\n}\n"
        );
        Ok(())
    }

    #[test]
    fn updates_yaml_version_without_rewriting_manifest() -> Result<(), String> {
        let contents =
            "name: demo\nversion: 1.0.0\ndependencies:\n  nested:\n    version: \"9.9.9\"\n";

        let updated = replace_manifest_version_preserving_style(
            Path::new("package.yaml"),
            contents,
            &Version::new(1, 1, 0),
        )?;

        assert_eq!(
            updated,
            "name: demo\nversion: 1.1.0\ndependencies:\n  nested:\n    version: \"9.9.9\"\n"
        );
        Ok(())
    }

    #[test]
    fn does_not_replace_nested_dependency_versions() -> Result<(), String> {
        let contents = "{\n  \"dependencies\": {\n    \"demo\": {\n      \"version\": \"9.9.9\"\n    }\n  },\n  \"version\": \"1.0.0\"\n}\n";

        let updated = replace_version_preserving_style(contents, &Version::new(1, 2, 0))?;

        assert!(updated.contains("\"version\": \"1.2.0\""));
        assert!(updated.contains("\"version\": \"9.9.9\""));
        Ok(())
    }
}
