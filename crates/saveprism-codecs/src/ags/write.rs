//! Writes an AGS save by rebuilding its self-describing header and appending the raw game state.

use saveprism_core::{Error, NodeId, Params, Result, SaveDoc, Value};

use crate::ags::{
    AGS_WRITE, SIGNATURE, VERSION_351, VERSION_363, VERSION_COMPONENTS_EX, parse_error,
};

pub(crate) fn encode(doc: &SaveDoc, _params: &Params) -> Result<Vec<u8>> {
    let root = doc.root();
    let signature = string(doc, root, "signature")?;
    if signature.as_bytes() != SIGNATURE {
        return Err(Error::new(
            AGS_WRITE,
            "the AGS signature is not the modern save signature",
        ));
    }
    let version = i32_of(doc, root, "version")?;
    if !(crate::ags::LOWEST_VERSION..=crate::ags::CURRENT_VERSION).contains(&version) {
        return Err(Error::new(
            AGS_WRITE,
            format!("AGS save format version {version} cannot be written"),
        ));
    }

    let env_id = entry(doc, root, "environment")
        .ok_or_else(|| Error::new(AGS_WRITE, "the AGS environment block is missing"))?;
    let desc_id = entry(doc, root, "description")
        .ok_or_else(|| Error::new(AGS_WRITE, "the AGS description block is missing"))?;
    let game_id = entry(doc, root, "game_data")
        .ok_or_else(|| Error::new(AGS_WRITE, "the AGS game data is missing"))?;
    let game_data = bytes(doc, game_id)?;

    let mut env = Vec::new();
    let env_size_pos = if version >= VERSION_351 {
        let pos = env.len();
        put_u32(&mut env, 0);
        Some(pos)
    } else {
        None
    };
    put_string(&mut env, &string(doc, env_id, "engine_name")?)?;
    put_string(&mut env, &string(doc, env_id, "engine_version")?)?;
    put_string(&mut env, &string(doc, env_id, "game_guid")?)?;
    put_string(&mut env, &string(doc, env_id, "game_title")?)?;
    put_string(&mut env, &string(doc, env_id, "main_data_filename")?)?;
    if version >= VERSION_COMPONENTS_EX {
        put_i32(&mut env, i32_of(doc, env_id, "main_data_version")?);
    }
    put_i32(&mut env, i32_of(doc, env_id, "color_depth")?);
    if version >= VERSION_351 {
        put_i32(&mut env, i32_of(doc, env_id, "legacy_id")?);
    }
    if let Some(pos) = env_size_pos {
        let size = u32::try_from(env.len())
            .map_err(|_| Error::new(AGS_WRITE, "the AGS environment block is too large"))?;
        patch_u32(&mut env, pos, size)?;
    }

    let image_id = entry(doc, desc_id, "image");
    let (image_flags, image_payload) = match image_id {
        Some(id) => {
            let flags = u32_of(doc, id, "flags")?;
            let payload = match entry(doc, id, "payload") {
                Some(payload_id) => bytes(doc, payload_id)?.to_vec(),
                None => Vec::new(),
            };
            (flags, payload)
        }
        None => (0, Vec::new()),
    };
    let mut user = Vec::new();
    put_string(&mut user, &string(doc, desc_id, "user_text")?)?;
    put_u32(&mut user, image_flags);
    user.extend_from_slice(&image_payload);

    let format_id = entry(doc, root, "format");
    let format_flags = match format_id {
        Some(id) => u32_of(doc, id, "flags")?,
        None => 0,
    };

    let mut out = Vec::new();
    out.extend_from_slice(SIGNATURE);
    put_i32(&mut out, version);
    if version >= VERSION_363 {
        let file_format_pos = out.len();
        for _ in 0..5 {
            put_u32(&mut out, 0);
        }
        let env_pos = out.len();
        out.extend_from_slice(&env);
        let user_pos = out.len();
        out.extend_from_slice(&user);
        let game_pos = out.len();
        out.extend_from_slice(game_data);
        let file_format_size = u32::try_from(env_pos.saturating_sub(file_format_pos))
            .map_err(|_| Error::new(AGS_WRITE, "the AGS file-format block is too large"))?;
        let env_offset = u32::try_from(env_pos)
            .map_err(|_| Error::new(AGS_WRITE, "the AGS environment offset does not fit"))?;
        let user_offset = u32::try_from(user_pos)
            .map_err(|_| Error::new(AGS_WRITE, "the AGS description offset does not fit"))?;
        let game_offset = u32::try_from(game_pos)
            .map_err(|_| Error::new(AGS_WRITE, "the AGS game-data offset does not fit"))?;
        patch_u32(&mut out, file_format_pos, file_format_size)?;
        patch_u32(&mut out, file_format_pos.saturating_add(4), format_flags)?;
        patch_u32(&mut out, file_format_pos.saturating_add(8), env_offset)?;
        patch_u32(&mut out, file_format_pos.saturating_add(12), user_offset)?;
        patch_u32(&mut out, file_format_pos.saturating_add(16), game_offset)?;
    } else {
        out.extend_from_slice(&env);
        out.extend_from_slice(&user);
        out.extend_from_slice(game_data);
    }
    Ok(out)
}

fn entry(doc: &SaveDoc, id: NodeId, key: &str) -> Option<NodeId> {
    let node = doc.get(id)?;
    let Value::Object(entries) = &node.value else {
        return None;
    };
    entries
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, child)| *child)
}

fn string(doc: &SaveDoc, id: NodeId, key: &str) -> Result<String> {
    let child = entry(doc, id, key)
        .ok_or_else(|| Error::new(AGS_WRITE, format!("the AGS field '{key}' is missing")))?;
    let node = doc
        .get(child)
        .ok_or_else(|| Error::new(AGS_WRITE, "the AGS document has an invalid node"))?;
    match &node.value {
        Value::Str(text) => Ok(text.clone()),
        _ => Err(Error::new(
            AGS_WRITE,
            format!("the AGS field '{key}' is not text"),
        )),
    }
}

fn int(doc: &SaveDoc, id: NodeId, key: &str) -> Result<i128> {
    let child = entry(doc, id, key)
        .ok_or_else(|| Error::new(AGS_WRITE, format!("the AGS field '{key}' is missing")))?;
    let node = doc
        .get(child)
        .ok_or_else(|| Error::new(AGS_WRITE, "the AGS document has an invalid node"))?;
    match &node.value {
        Value::Int { v, .. } => Ok(*v),
        _ => Err(Error::new(
            AGS_WRITE,
            format!("the AGS field '{key}' is not an integer"),
        )),
    }
}

fn i32_of(doc: &SaveDoc, id: NodeId, key: &str) -> Result<i32> {
    let value = int(doc, id, key)?;
    i32::try_from(value).map_err(|_| {
        Error::new(
            AGS_WRITE,
            format!("the AGS field '{key}' does not fit int32"),
        )
    })
}

fn u32_of(doc: &SaveDoc, id: NodeId, key: &str) -> Result<u32> {
    let value = int(doc, id, key)?;
    u32::try_from(value).map_err(|_| {
        Error::new(
            AGS_WRITE,
            format!("the AGS field '{key}' does not fit uint32"),
        )
    })
}

fn bytes(doc: &SaveDoc, id: NodeId) -> Result<&[u8]> {
    let node = doc
        .get(id)
        .ok_or_else(|| Error::new(AGS_WRITE, "the AGS document has an invalid node"))?;
    match &node.value {
        Value::Bytes(data) => Ok(data),
        _ => Err(Error::new(AGS_WRITE, "an AGS byte field is not raw bytes")),
    }
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn patch_u32(out: &mut [u8], offset: usize, value: u32) -> Result<()> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| parse_error("the AGS header offset is too large"))?;
    let slot = out
        .get_mut(offset..end)
        .ok_or_else(|| parse_error("the AGS header is shorter than its offsets"))?;
    slot.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn put_string(out: &mut Vec<u8>, text: &str) -> Result<()> {
    let length = i32::try_from(text.len())
        .map_err(|_| Error::new(AGS_WRITE, "an AGS string is too long"))?;
    put_i32(out, length);
    out.extend_from_slice(text.as_bytes());
    Ok(())
}
