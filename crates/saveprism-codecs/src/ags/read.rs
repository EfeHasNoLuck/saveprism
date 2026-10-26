//! Reads the header, description and component list of an AGS save.

use flate2::read::ZlibDecoder;
use saveprism_core::limits::len_u64;
use saveprism_core::{
    ByteSpan, DecodeCtx, IntWidth, Node, NodeFlags, NodeId, Params, Result, SaveDoc, TypeHint,
    Value,
};

use crate::ags::{
    AGS_PARSE, AGS_UNSUPPORTED, HINT_PREFIX, LEGACY_SIGNATURE, LOWEST_VERSION, SIGNATURE,
    VERSION_351, VERSION_363, VERSION_COMPONENTS_EX, parse_error,
};

const MAX_COMPONENTS: usize = 100_000;

pub(crate) fn read_all(input: &[u8], ctx: &DecodeCtx<'_>) -> Result<(SaveDoc, Params)> {
    if input.starts_with(LEGACY_SIGNATURE) && !input.starts_with(SIGNATURE) {
        return Err(saveprism_core::Error::new(
            AGS_UNSUPPORTED,
            "AGS saves before 3.5 (the legacy signature) are not supported",
        ));
    }
    if !input.starts_with(SIGNATURE) {
        return Err(parse_error("not an Adventure Game Studio saved game"));
    }

    let mut cursor = Cursor::new(input, SIGNATURE.len());
    let version = cursor.i32()?;
    if !(LOWEST_VERSION..=crate::ags::CURRENT_VERSION).contains(&version) {
        return Err(saveprism_core::Error::new(
            AGS_UNSUPPORTED,
            format!("AGS save format version {version} is not supported"),
        ));
    }

    let mut format = None;
    let mut env_offset = cursor.pos;
    let mut user_desc_offset = None;
    let mut game_data_offset = None;
    if version >= VERSION_363 {
        let file_format_offset = cursor.pos;
        let file_format_size = cursor.u32()?;
        let flags = cursor.u32()?;
        let env = cursor.u32()?;
        let user = cursor.u32()?;
        let game = cursor.u32()?;
        format = Some(Format {
            offset: file_format_offset,
            size: file_format_size,
            flags,
            env_offset: env,
            user_desc_offset: user,
            game_data_offset: game,
        });
        if env > 0 {
            env_offset = usize::try_from(env)
                .map_err(|_| parse_error("the AGS environment offset does not fit"))?;
        }
        if user > 0 {
            user_desc_offset = Some(
                usize::try_from(user)
                    .map_err(|_| parse_error("the AGS user-description offset does not fit"))?,
            );
        }
        if game > 0 {
            game_data_offset = Some(
                usize::try_from(game)
                    .map_err(|_| parse_error("the AGS game-data offset does not fit"))?,
            );
        }
    }
    if env_offset > input.len() {
        return Err(parse_error(
            "the AGS environment offset is outside the file",
        ));
    }

    let mut env = Cursor::new(input, env_offset);
    let env_header_size = if version >= VERSION_351 {
        let size = env.u32()?;
        if size < 4 {
            return Err(parse_error("the AGS environment block is too small"));
        }
        Some(size)
    } else {
        None
    };
    let engine_name = env.string()?;
    let engine_version = env.string()?;
    let game_guid = env.string()?;
    let game_title = env.string()?;
    let main_data_filename = env.string()?;
    let main_data_version = if version >= VERSION_COMPONENTS_EX {
        Some(env.i32()?)
    } else {
        None
    };
    let color_depth = env.i32()?;
    let legacy_id = if version >= VERSION_351 {
        Some(env.i32()?)
    } else {
        None
    };
    let env_end = env.pos;

    let user_pos = match user_desc_offset {
        Some(offset) => offset,
        None if version >= VERSION_351 => env_offset
            .checked_add(
                usize::try_from(env_header_size.unwrap_or(0))
                    .map_err(|_| parse_error("the AGS environment size does not fit"))?,
            )
            .map(|end| end.min(input.len()))
            .unwrap_or(env_end),
        None => env_end,
    };
    let mut user = Cursor::new(input, user_pos.min(input.len()));
    let user_text = user.string()?;
    let image_flags = user.u32()?;
    let image_start = user.pos;
    let (image_width, image_height, image_depth) = if image_flags & 1 != 0 {
        let width = user.i32()?;
        let height = user.i32()?;
        let depth = user.i32()?;
        if width <= 0 || height <= 0 || !(1..=64).contains(&depth) {
            return Err(parse_error("the AGS screenshot has invalid dimensions"));
        }
        if image_flags & 2 != 0 {
            let _reserved = user.u32()?;
            let compressed = user.u32()?;
            let size = usize::try_from(compressed)
                .map_err(|_| parse_error("the AGS screenshot size does not fit"))?;
            user.take(size)?;
        } else {
            let bpp = (depth as usize)
                .checked_add(7)
                .ok_or_else(|| parse_error("the AGS screenshot depth is too large"))?
                / 8;
            let pixels = (width as usize)
                .checked_mul(height as usize)
                .and_then(|pixels| pixels.checked_mul(bpp))
                .ok_or_else(|| parse_error("the AGS screenshot is too large"))?;
            user.take(pixels)?;
        }
        (Some(width), Some(height), Some(depth))
    } else {
        (None, None, None)
    };
    let image_end = user.pos;
    let image_payload = input
        .get(image_start..image_end)
        .ok_or_else(|| parse_error("the AGS screenshot is truncated"))?
        .to_vec();

    let game_pos = match game_data_offset {
        Some(offset) => offset,
        None => user.pos,
    };
    if game_pos > input.len() {
        return Err(parse_error("the AGS game-data offset is outside the file"));
    }
    let game_data = input
        .get(game_pos..)
        .ok_or_else(|| parse_error("the AGS game data is missing"))?;

    let mut doc = ctx.new_doc();

    let signature_id = {
        let mut node = Node::new(Value::Str(
            std::str::from_utf8(SIGNATURE)
                .map_err(|_| parse_error("the AGS signature is not text"))?
                .to_owned(),
        ));
        node.type_hint = Some(hint("signature"));
        node.flags.insert(NodeFlags::READ_ONLY);
        doc.push(node)?
    };
    let version_id = push_int(
        &mut doc,
        "version",
        i128::from(version),
        IntWidth::I32,
        true,
    )?;

    let format_id = match format {
        Some(format) => {
            let entries = vec![
                (
                    "file_format_offset".to_owned(),
                    push_u32(&mut doc, i128::from(format.offset as u64), true)?,
                ),
                (
                    "file_format_size".to_owned(),
                    push_u32(&mut doc, i128::from(format.size), true)?,
                ),
                (
                    "flags".to_owned(),
                    push_u32(&mut doc, i128::from(format.flags), true)?,
                ),
                (
                    "env_info_offset".to_owned(),
                    push_u32(&mut doc, i128::from(format.env_offset), true)?,
                ),
                (
                    "user_desc_offset".to_owned(),
                    push_u32(&mut doc, i128::from(format.user_desc_offset), true)?,
                ),
                (
                    "game_data_offset".to_owned(),
                    push_u32(&mut doc, i128::from(format.game_data_offset), true)?,
                ),
            ];
            Some(push_object(&mut doc, "format", entries, true)?)
        }
        None => None,
    };

    let mut env_entries = Vec::new();
    if let Some(size) = env_header_size {
        env_entries.push((
            "header_size".to_owned(),
            push_u32(&mut doc, i128::from(size), true)?,
        ));
    }
    env_entries.extend([
        (
            "engine_name".to_owned(),
            push_str(&mut doc, "engine_name", engine_name, false)?,
        ),
        (
            "engine_version".to_owned(),
            push_str(&mut doc, "engine_version", engine_version, false)?,
        ),
        (
            "game_guid".to_owned(),
            push_str(&mut doc, "game_guid", game_guid, false)?,
        ),
        (
            "game_title".to_owned(),
            push_str(&mut doc, "game_title", game_title, false)?,
        ),
        (
            "main_data_filename".to_owned(),
            push_str(&mut doc, "main_data_filename", main_data_filename, false)?,
        ),
    ]);
    if let Some(value) = main_data_version {
        env_entries.push((
            "main_data_version".to_owned(),
            push_int(
                &mut doc,
                "main_data_version",
                i128::from(value),
                IntWidth::I32,
                true,
            )?,
        ));
    }
    env_entries.push((
        "color_depth".to_owned(),
        push_int(
            &mut doc,
            "color_depth",
            i128::from(color_depth),
            IntWidth::I32,
            true,
        )?,
    ));
    if let Some(value) = legacy_id {
        env_entries.push((
            "legacy_id".to_owned(),
            push_int(
                &mut doc,
                "legacy_id",
                i128::from(value),
                IntWidth::I32,
                true,
            )?,
        ));
    }
    let env_id = push_object(&mut doc, "environment", env_entries, true)?;

    let mut desc_entries = vec![(
        "user_text".to_owned(),
        push_str(&mut doc, "user_text", user_text, false)?,
    )];
    let mut image_entries = vec![(
        "flags".to_owned(),
        push_u32(&mut doc, i128::from(image_flags), true)?,
    )];
    if let (Some(width), Some(height), Some(depth)) = (image_width, image_height, image_depth) {
        image_entries.push((
            "width".to_owned(),
            push_int(&mut doc, "width", i128::from(width), IntWidth::I32, true)?,
        ));
        image_entries.push((
            "height".to_owned(),
            push_int(&mut doc, "height", i128::from(height), IntWidth::I32, true)?,
        ));
        image_entries.push((
            "depth".to_owned(),
            push_int(&mut doc, "depth", i128::from(depth), IntWidth::I32, true)?,
        ));
    }
    let payload = {
        let mut node = Node::new(Value::Bytes(image_payload));
        node.type_hint = Some(hint("image-bytes"));
        node.flags.insert(NodeFlags::READ_ONLY);
        doc.push(node)?
    };
    image_entries.push(("payload".to_owned(), payload));
    let image_id = push_object(&mut doc, "image", image_entries, true)?;
    desc_entries.push(("image".to_owned(), image_id));
    let desc_id = push_object(&mut doc, "description", desc_entries, true)?;

    let parsed = scan_components(input, game_pos, version, ctx);
    let components_id = if let Some((components, _)) = &parsed {
        let mut items = Vec::with_capacity(components.len());
        for component in components {
            let data = input
                .get(component.data_start..component.data_end)
                .unwrap_or_default();
            let entries = vec![
                (
                    "name".to_owned(),
                    push_str(&mut doc, "name", component.name.clone(), true)?,
                ),
                (
                    "version".to_owned(),
                    push_int(
                        &mut doc,
                        "version",
                        i128::from(component.version),
                        IntWidth::I32,
                        true,
                    )?,
                ),
                (
                    "flags".to_owned(),
                    push_u32(&mut doc, i128::from(component.flags), true)?,
                ),
                (
                    "data_offset".to_owned(),
                    push_u32(&mut doc, i128::from(component.data_start as u64), true)?,
                ),
                (
                    "data_size".to_owned(),
                    push_u32(&mut doc, i128::from(component.data_size), true)?,
                ),
                (
                    "uncompressed_size".to_owned(),
                    push_u32(&mut doc, i128::from(component.uncompressed_size), true)?,
                ),
                (
                    "checksum".to_owned(),
                    push_u32(&mut doc, i128::from(component.checksum), true)?,
                ),
                (
                    "data".to_owned(),
                    push_read_only_bytes(&mut doc, "component-data", data.to_vec())?,
                ),
            ];
            items.push(push_object(&mut doc, "component", entries, true)?);
        }
        let mut node = Node::new(Value::Array(items));
        node.type_hint = Some(hint("components"));
        node.flags.insert(NodeFlags::READ_ONLY);
        Some(doc.push(node)?)
    } else {
        None
    };

    let mut globals_id = None;
    if let Some(script) = parsed.as_ref().and_then(|(_, script)| script.as_ref()) {
        let mut values = Vec::with_capacity(script.global.len() / 4);
        for (index, bytes) in script.global.as_chunks::<4>().0.iter().enumerate() {
            let value = i32::from_le_bytes(*bytes);
            let offset = index.saturating_mul(4);
            let entries = vec![
                (
                    "offset".to_owned(),
                    push_u32(&mut doc, i128::from(offset as u64), true)?,
                ),
                (
                    "value".to_owned(),
                    push_int(
                        &mut doc,
                        "script-global",
                        i128::from(value),
                        IntWidth::I32,
                        true,
                    )?,
                ),
            ];
            values.push(push_object(&mut doc, "script-global", entries, true)?);
        }
        let mut node = Node::new(Value::Array(values));
        node.type_hint = Some(hint("script-globals"));
        node.flags.insert(NodeFlags::READ_ONLY);
        globals_id = Some(doc.push(node)?);
    }

    let game_id = push_read_only_bytes(&mut doc, "game-data", game_data.to_vec())?;

    let mut root_entries = vec![
        ("signature".to_owned(), signature_id),
        ("version".to_owned(), version_id),
        ("environment".to_owned(), env_id),
        ("description".to_owned(), desc_id),
        ("game_data".to_owned(), game_id),
    ];
    if let Some(id) = format_id {
        root_entries.push(("format".to_owned(), id));
    }
    if let Some(id) = components_id {
        root_entries.push(("components".to_owned(), id));
    }
    if let Some(id) = globals_id {
        root_entries.push(("script_globals".to_owned(), id));
    }
    let root = push_object(&mut doc, "save", root_entries, true)?;
    doc.set_root(root);

    let mut params = Params::new();
    params.set("original", input.to_vec());
    Ok((doc, params))
}

struct Format {
    offset: usize,
    size: u32,
    flags: u32,
    env_offset: u32,
    user_desc_offset: u32,
    game_data_offset: u32,
}

struct Component {
    name: String,
    version: i32,
    flags: u32,
    data_start: usize,
    data_end: usize,
    data_size: u32,
    uncompressed_size: u32,
    checksum: u32,
}

struct Script {
    global: Vec<u8>,
}

fn scan_components(
    input: &[u8],
    start: usize,
    version: i32,
    ctx: &DecodeCtx<'_>,
) -> Option<(Vec<Component>, Option<Script>)> {
    let (name, mut pos, closing) = read_tag(input, start)?;
    if closing || name != "Components" {
        return None;
    }
    let mut components = Vec::new();
    let mut script = None;
    while components.len() < MAX_COMPONENTS {
        let (name, next, closing) = read_tag(input, pos)?;
        if closing {
            if name == "Components" {
                return Some((components, script));
            }
            return None;
        }
        pos = next;
        let mut cursor = Cursor::new(input, pos);
        let (component_version, data_size, uncompressed_size, checksum, flags) =
            if version >= VERSION_363 {
                let _header_size = cursor.u32().ok()?;
                let flags = cursor.u32().ok()?;
                let component_version = cursor.i32().ok()?;
                let data_size = cursor.u32().ok()?;
                let uncompressed_size = cursor.u32().ok()?;
                let checksum = cursor.u32().ok()?;
                (
                    component_version,
                    data_size,
                    uncompressed_size,
                    checksum,
                    flags,
                )
            } else if version >= VERSION_COMPONENTS_EX {
                let component_version = cursor.i32().ok()?;
                let data_size = cursor.u32().ok()?;
                let _reserved = cursor.u32().ok()?;
                (component_version, data_size, data_size, 0, 0)
            } else {
                let component_version = cursor.i32().ok()?;
                let data_size = cursor.u32().ok()?;
                (component_version, data_size, data_size, 0, 0)
            };
        let data_start = cursor.pos;
        let data_end = data_start.checked_add(usize::try_from(data_size).ok()?)?;
        if data_end > input.len() {
            return None;
        }
        let data = input.get(data_start..data_end)?;
        if name == "Script Modules" && script.is_none() {
            let decoded = if flags & 1 != 0 {
                let decoder = ZlibDecoder::new(data);
                ctx.read_bounded(decoder, len_u64(input.len()), AGS_PARSE)
                    .ok()?
            } else {
                data.to_vec()
            };
            script = parse_script_data(&decoded, component_version);
        }
        let (close_name, after_data, close) = read_tag(input, data_end)?;
        if !close || close_name != name {
            return None;
        }
        components.push(Component {
            name,
            version: component_version,
            flags,
            data_start,
            data_end,
            data_size,
            uncompressed_size,
            checksum,
        });
        pos = after_data;
    }
    None
}

fn parse_script_data(data: &[u8], component_version: i32) -> Option<Script> {
    let mut cursor = Cursor::new(data, 0);
    let global_len = cursor.i32().ok()?;
    if global_len < 0 {
        return None;
    }
    let global = cursor
        .take(usize::try_from(global_len).ok()?)
        .ok()?
        .to_vec();
    let modules = cursor.i32().ok()?;
    if modules < 0 {
        return None;
    }
    for _ in 0..modules {
        if component_version >= 3_060_200 {
            let _name = cursor.string().ok()?;
        }
        let length = cursor.i32().ok()?;
        if length < 0 {
            return None;
        }
        cursor.take(usize::try_from(length).ok()?).ok()?;
    }
    Some(Script { global })
}

fn read_tag(input: &[u8], start: usize) -> Option<(String, usize, bool)> {
    if input.get(start).copied() != Some(b'<') {
        return None;
    }
    let rest = input.get(start.saturating_add(1)..)?;
    let close = rest.iter().position(|byte| *byte == b'>')?;
    let tag = rest.get(..close)?;
    let (closing, name) = match tag.first() {
        Some(b'/') => (true, tag.get(1..)?),
        _ => (false, tag),
    };
    let name = std::str::from_utf8(name).ok()?.to_owned();
    Some((name, start.saturating_add(close).saturating_add(2), closing))
}

fn hint(name: &str) -> TypeHint {
    TypeHint::new(format!("{HINT_PREFIX}{name}"))
}

fn push_str(doc: &mut SaveDoc, name: &str, text: String, read_only: bool) -> Result<NodeId> {
    let mut node = Node::new(Value::Str(text));
    node.type_hint = Some(hint(name));
    if read_only {
        node.flags.insert(NodeFlags::READ_ONLY);
    }
    doc.push(node)
}

fn push_int(
    doc: &mut SaveDoc,
    name: &str,
    value: i128,
    width: IntWidth,
    read_only: bool,
) -> Result<NodeId> {
    let mut node = Node::new(Value::Int { v: value, width });
    node.type_hint = Some(hint(name));
    if read_only {
        node.flags.insert(NodeFlags::READ_ONLY);
    }
    doc.push(node)
}

fn push_u32(doc: &mut SaveDoc, value: i128, read_only: bool) -> Result<NodeId> {
    push_int(doc, "u32", value, IntWidth::U32, read_only)
}

fn push_object(
    doc: &mut SaveDoc,
    name: &str,
    entries: Vec<(String, NodeId)>,
    read_only: bool,
) -> Result<NodeId> {
    let mut node = Node::new(Value::Object(entries));
    node.type_hint = Some(hint(name));
    if read_only {
        node.flags.insert(NodeFlags::READ_ONLY);
    }
    doc.push(node)
}

fn push_read_only_bytes(doc: &mut SaveDoc, name: &str, bytes: Vec<u8>) -> Result<NodeId> {
    let mut node = Node::new(Value::Bytes(bytes));
    node.type_hint = Some(hint(name));
    node.flags.insert(NodeFlags::READ_ONLY);
    node.span = Some(ByteSpan { start: 0, end: 0 });
    doc.push(node)
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8], pos: usize) -> Self {
        Self { data, pos }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(count)
            .filter(|end| *end <= self.data.len())
            .ok_or_else(|| parse_error("the AGS save is truncated"))?;
        let slice = self
            .data
            .get(self.pos..end)
            .ok_or_else(|| parse_error("the AGS save is truncated"))?;
        self.pos = end;
        Ok(slice)
    }

    fn u32(&mut self) -> Result<u32> {
        let bytes = self.take(4)?;
        let array =
            <[u8; 4]>::try_from(bytes).map_err(|_| parse_error("the AGS save is truncated"))?;
        Ok(u32::from_le_bytes(array))
    }

    fn i32(&mut self) -> Result<i32> {
        let bytes = self.take(4)?;
        let array =
            <[u8; 4]>::try_from(bytes).map_err(|_| parse_error("the AGS save is truncated"))?;
        Ok(i32::from_le_bytes(array))
    }

    fn string(&mut self) -> Result<String> {
        let length = self.i32()?;
        if length < 0 {
            return Err(parse_error("an AGS string has a negative length"));
        }
        let bytes = self.take(
            usize::try_from(length)
                .map_err(|_| parse_error("an AGS string length does not fit this platform"))?,
        )?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| parse_error("an AGS string is not valid UTF-8"))
    }
}
