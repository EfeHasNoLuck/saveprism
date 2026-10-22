//! SavePrism command line tool: `inspect`, `decode` and `encode`.
//!
//! [`run`] takes the arguments and the output streams so it can be tested in process.
//! Exit codes: 0 success, 1 error (report on stderr), 2 usage error (clap).

use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use saveprism_codecs::es3::ES3_WRONG_PASSWORD;
use saveprism_core::detect::{DetectOptions, Detection, detect};
use saveprism_core::guard::{GuardReport, Verdict, check_round_trip};
use saveprism_core::io::{WriteOptions, default_backup_root, safe_write};
use saveprism_core::{
    Codec, DecodeCtx, Error, ErrorCode, Limits, Params, Recipe, Registry, Result, Step, codes,
    pipeline,
};

/// The JSON codec: documents are read and written as JSON text with it.
const JSON_CODEC: &str = "format.json";
/// Codecs whose input bytes already are the JSON text, so `decode` writes those bytes as the JSON file.
const JSON_TEXT_CODECS: [&str; 3] = ["format.json", "format.rpgmaker-json", "format.es3"];
/// Codecs that `decode` and `encode` accept: their document is a JSON tree.
const JSON_TREE_CODECS: [&str; 4] = [
    "format.json",
    "format.rpgmaker-json",
    "format.es3",
    "format.gvas",
];
/// The layer that `keys try` checks candidate passwords with.
const ES3_AES_LAYER: &str = "crypto.es3-aes";
/// Most candidate passwords `keys try` accepts in one run.
const MAX_CANDIDATES: usize = 100_000;
/// Largest candidate list file, in bytes (16 MB).
const MAX_CANDIDATE_FILE_BYTES: u64 = 16 * 1024 * 1024;
/// The candidate list cannot be used (empty, not UTF-8, too large or too many candidates).
const KEYS_BAD_LIST: ErrorCode = ErrorCode::new("SP-CLI-001");
/// No candidate opened the file.
const KEYS_NOT_FOUND: ErrorCode = ErrorCode::new("SP-CLI-002");

#[derive(Parser, Debug)]
#[command(
    name = "saveprism",
    version,
    about = "Inspect, decode and encode game save files"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Detect how a file is built and check that it can be written back unchanged.
    Inspect {
        /// The save file.
        file: PathBuf,
        /// Print machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Decode a file to JSON and a recipe (`<output>.recipe.toml`).
    Decode {
        /// The save file.
        file: PathBuf,
        /// Where the JSON is written.
        #[arg(short, long)]
        output: PathBuf,
        /// Directory for backups of overwritten files.
        #[arg(long)]
        backup_dir: Option<PathBuf>,
    },
    /// Encode JSON back to a save file using a recipe.
    Encode {
        /// The JSON file.
        file: PathBuf,
        /// The recipe written by `decode`.
        #[arg(long)]
        recipe: PathBuf,
        /// The save file to write.
        #[arg(short, long)]
        output: PathBuf,
        /// Directory for backups of overwritten files.
        #[arg(long)]
        backup_dir: Option<PathBuf>,
    },
    /// Work with the passwords of encrypted saves.
    Keys {
        #[command(subcommand)]
        action: KeysCommand,
    },
}

#[derive(Subcommand, Debug)]
enum KeysCommand {
    /// Try candidate passwords on an encrypted Easy Save 3 file and print the first one that opens it.
    Try {
        /// The encrypted save file.
        file: PathBuf,
        /// UTF-8 text with one candidate per line. Empty lines and lines starting with `#` are skipped.
        #[arg(long)]
        candidates: PathBuf,
    },
}

/// Runs the tool with `args` (the first item is the program name). Returns the exit code.
pub fn run<I: IntoIterator<Item = OsString>>(
    args: I,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(usage) => {
            let text = usage.render().to_string();
            // Help and version go to stdout, real usage errors to stderr.
            let _ = if usage.use_stderr() {
                write!(err, "{text}")
            } else {
                write!(out, "{text}")
            };
            return usage.exit_code();
        }
    };
    match dispatch(cli.command, out) {
        Ok(()) => 0,
        Err(error) => {
            let _ = writeln!(err, "{}", error.report());
            1
        }
    }
}

fn dispatch(command: Command, out: &mut dyn Write) -> Result<()> {
    let mut reg = Registry::default();
    saveprism_codecs::register_all(&mut reg);
    let limits = Limits::default();
    match command {
        Command::Inspect { file, json } => inspect(&file, json, &reg, &limits, out),
        Command::Decode {
            file,
            output,
            backup_dir,
        } => decode(&file, &output, backup_dir, &reg, &limits, out),
        Command::Encode {
            file,
            recipe,
            output,
            backup_dir,
        } => encode(&file, &recipe, &output, backup_dir, &reg, &limits, out),
        Command::Keys { action } => match action {
            KeysCommand::Try { file, candidates } => {
                keys_try(&file, &candidates, &reg, &limits, out)
            }
        },
    }
}

// --- helpers ---

fn say(out: &mut dyn Write, text: &str) -> Result<()> {
    out.write_all(text.as_bytes()).map_err(|e| {
        Error::new(
            codes::IO_TEMP_WRITE,
            format!("cannot write to the output stream: {e}"),
        )
        .with_source(e)
    })
}

fn read_error(path: &Path, source: std::io::Error) -> Error {
    Error::new(
        codes::IO_READ,
        format!(
            "cannot read \"{}\": {source}. Check that the file exists and can be read.",
            path.display()
        ),
    )
    .with_source(source)
}

/// Reads a file after checking its size against the input limit.
fn read_file(path: &Path, limits: &Limits) -> Result<Vec<u8>> {
    let meta = fs::metadata(path).map_err(|e| read_error(path, e))?;
    limits
        .check_input(meta.len())
        .map_err(|e| e.context(format!("while reading {}", path.display())))?;
    fs::read(path).map_err(|e| read_error(path, e))
}

fn detect_file(path: &Path, bytes: &[u8], reg: &Registry, limits: &Limits) -> Result<Detection> {
    let opts = DetectOptions {
        file_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned),
        ..DetectOptions::default()
    };
    detect(bytes, reg, limits, &opts)
        .map_err(|e| e.context(format!("while detecting the format of {}", path.display())))
}

fn json_codec(reg: &Registry) -> Result<&dyn Codec> {
    reg.codec(JSON_CODEC).ok_or_else(|| {
        Error::new(
            codes::CORE_UNKNOWN_STEP,
            format!("no codec is registered as \"{JSON_CODEC}\""),
        )
    })
}

fn require_json_codec(recipe: &Recipe) -> Result<()> {
    match recipe.steps.last() {
        Some(step) if JSON_TREE_CODECS.contains(&step.kind.as_str()) => Ok(()),
        Some(step) => Err(Error::new(
            codes::CORE_INVALID_RECIPE,
            format!(
                "the file uses the format \"{}\", but this version only handles {}. \
                 Use `saveprism inspect` to look at the file instead.",
                step.kind,
                JSON_TREE_CODECS.join(", ")
            ),
        )),
        None => Err(Error::new(
            codes::CORE_INVALID_RECIPE,
            "the recipe has no steps",
        )),
    }
}

fn backup_options(backup_dir: Option<PathBuf>) -> Result<WriteOptions> {
    let root = backup_dir.or_else(default_backup_root).ok_or_else(|| {
        Error::new(
            codes::IO_NO_BACKUP_ROOT,
            "no backup directory is known. Pass one with --backup-dir <directory>.",
        )
    })?;
    Ok(WriteOptions::new(root))
}

fn verdict_name(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Exact => "exact",
        Verdict::Semantic => "semantic",
        Verdict::Different => "different",
    }
}

fn arrow(kinds: &[&str]) -> String {
    kinds.join(" -> ")
}

/// A JSON string literal with the needed escapes.
fn json_str(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('"');
    for ch in text.chars() {
        match ch {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(quoted, "\\u{:04x}", u32::from(c));
            }
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

fn json_kinds(kinds: &[&str]) -> String {
    let items: Vec<String> = kinds.iter().map(|kind| json_str(kind)).collect();
    format!("[{}]", items.join(","))
}

// --- inspect ---

fn inspect(
    path: &Path,
    json: bool,
    reg: &Registry,
    limits: &Limits,
    out: &mut dyn Write,
) -> Result<()> {
    let bytes = read_file(path, limits)?;
    let detection = detect_file(path, &bytes, reg, limits)?;
    let best = &detection.best;
    let report = check_round_trip(&bytes, &best.recipe, reg, limits).map_err(|e| {
        e.context(format!(
            "while checking the round trip of {}",
            path.display()
        ))
    })?;
    let text = if json {
        inspect_json(path, bytes.len(), &detection, &report)
    } else {
        inspect_text(path, bytes.len(), &detection, &report)
    };
    say(out, &text)
}

fn inspect_text(path: &Path, size: usize, detection: &Detection, report: &GuardReport) -> String {
    let best = &detection.best;
    let mut text = String::new();
    let _ = writeln!(text, "file:        {}", path.display());
    let _ = writeln!(text, "size:        {size} bytes");
    let _ = writeln!(text, "recipe:      {}", arrow(&best.recipe.kinds()));
    let _ = writeln!(text, "confidence:  {:.2}", best.confidence.value());
    let _ = writeln!(
        text,
        "round trip:  {} (editable: {})",
        verdict_name(report.overall),
        if report.editable { "yes" } else { "no" }
    );
    for step in &report.steps {
        let _ = writeln!(
            text,
            "  step {} {}: {} ({})",
            step.index + 1,
            step.kind,
            verdict_name(step.verdict),
            step.detail
        );
    }
    for alternative in &detection.alternatives {
        let _ = writeln!(
            text,
            "alternative: {} (confidence {:.2})",
            arrow(&alternative.recipe.kinds()),
            alternative.confidence.value()
        );
    }
    text
}

fn inspect_json(path: &Path, size: usize, detection: &Detection, report: &GuardReport) -> String {
    let best = &detection.best;
    let steps: Vec<String> = report
        .steps
        .iter()
        .map(|step| {
            format!(
                "{{\"kind\":{},\"verdict\":{},\"detail\":{}}}",
                json_str(&step.kind),
                json_str(verdict_name(step.verdict)),
                json_str(&step.detail)
            )
        })
        .collect();
    let alternatives: Vec<String> = detection
        .alternatives
        .iter()
        .map(|alternative| {
            format!(
                "{{\"recipe\":{},\"confidence\":{:.3}}}",
                json_kinds(&alternative.recipe.kinds()),
                alternative.confidence.value()
            )
        })
        .collect();
    format!(
        "{{\"file\":{},\"size\":{size},\"recipe\":{},\"confidence\":{:.3},\
         \"round_trip\":{{\"overall\":{},\"editable\":{},\"steps\":[{}]}},\
         \"alternatives\":[{}]}}\n",
        json_str(&path.display().to_string()),
        json_kinds(&best.recipe.kinds()),
        best.confidence.value(),
        json_str(verdict_name(report.overall)),
        report.editable,
        steps.join(","),
        alternatives.join(",")
    )
}

// --- decode ---

fn verify_equal(expected: &[u8]) -> impl Fn(&[u8]) -> Result<()> + '_ {
    move |written: &[u8]| {
        if written == expected {
            Ok(())
        } else {
            // Not IO_VERIFY_RESTORED: `safe_write` sets that code only after it restored a backup.
            Err(Error::new(
                codes::CORE_EDIT_FAILED,
                "the written file differs from the bytes that were meant to be written",
            ))
        }
    }
}

/// The JSON file for a decoded save. Codecs that read JSON text give their input back unchanged;
/// the others (`format.gvas`) are written by the JSON codec from the document and the codec's
/// parameters, which is the same write the codec does on `encode`.
fn json_text(decoded: &pipeline::DecodeOutput, reg: &Registry) -> Result<Vec<u8>> {
    let codec_step = decoded.recipe.steps.last().ok_or_else(|| {
        Error::new(
            codes::CORE_INVALID_RECIPE,
            "the decoded recipe has no steps",
        )
    })?;
    if JSON_TEXT_CODECS.contains(&codec_step.kind.as_str()) {
        let input = decoded.frames.last().ok_or_else(|| {
            Error::new(
                codes::CORE_INVALID_RECIPE,
                "the decoded result has no frames",
            )
        })?;
        return Ok(input.clone());
    }
    json_codec(reg)?.encode(&decoded.doc, &codec_step.params)
}

fn decode(
    path: &Path,
    output: &Path,
    backup_dir: Option<PathBuf>,
    reg: &Registry,
    limits: &Limits,
    out: &mut dyn Write,
) -> Result<()> {
    let bytes = read_file(path, limits)?;
    let detection = detect_file(path, &bytes, reg, limits)?;
    require_json_codec(&detection.best.recipe)?;
    let decoded = pipeline::decode(&bytes, &detection.best.recipe, reg, limits)?;
    let json = json_text(&decoded, reg)?;
    let recipe_text = decoded.recipe.to_toml()?;
    let recipe_path = output.with_extension("recipe.toml");
    let opts = backup_options(backup_dir)?;

    safe_write(output, &json, &opts, &verify_equal(&json))?;
    safe_write(
        &recipe_path,
        recipe_text.as_bytes(),
        &opts,
        &verify_equal(recipe_text.as_bytes()),
    )?;
    say(
        out,
        &format!(
            "decoded {} ({})\nwrote {} ({} bytes)\nwrote {}\n",
            path.display(),
            arrow(&decoded.recipe.kinds()),
            output.display(),
            json.len(),
            recipe_path.display()
        ),
    )
}

// --- encode ---

fn encode(
    json_path: &Path,
    recipe_path: &Path,
    output: &Path,
    backup_dir: Option<PathBuf>,
    reg: &Registry,
    limits: &Limits,
    out: &mut dyn Write,
) -> Result<()> {
    let recipe_bytes = read_file(recipe_path, limits)?;
    let recipe_text = String::from_utf8(recipe_bytes).map_err(|e| {
        Error::new(
            codes::CORE_RECIPE_FORMAT,
            format!(
                "the recipe file {} is not valid UTF-8. Use the recipe written by `saveprism decode`.",
                recipe_path.display()
            ),
        )
        .with_source(e)
    })?;
    let recipe = Recipe::from_toml(&recipe_text)?;
    recipe.validate(reg, limits)?;
    require_json_codec(&recipe)?;

    // Read the JSON with the codec alone; the recipe's own parameters write it back.
    let json = read_file(json_path, limits)?;
    let json_recipe = Recipe::new(vec![Step::bare(JSON_CODEC)]);
    let doc = pipeline::decode(&json, &json_recipe, reg, limits)
        .map_err(|e| e.context(format!("while reading {}", json_path.display())))?
        .doc;
    let bytes = pipeline::encode(&doc, &recipe, reg)?;

    let opts = backup_options(backup_dir)?;
    let verify = |written: &[u8]| -> Result<()> {
        let again = pipeline::decode(written, &recipe, reg, limits)?;
        // The user edits JSON text, so compare the JSON views: codecs such as `format.es3` add
        // type hints on decode that the plain JSON input does not carry.
        let same = again.doc.tree_eq(&doc) || {
            let params = recipe.steps.last().map(|step| &step.params);
            match (params, json_codec(reg)) {
                (Some(params), Ok(codec)) => {
                    match (codec.encode(&again.doc, params), codec.encode(&doc, params)) {
                        (Ok(written_view), Ok(input_view)) => written_view == input_view,
                        _ => false,
                    }
                }
                _ => false,
            }
        };
        if same {
            Ok(())
        } else {
            // Not IO_VERIFY_RESTORED: `safe_write` sets that code only after it restored a backup.
            Err(Error::new(
                codes::CORE_EDIT_FAILED,
                "the written file decodes to a different document",
            ))
        }
    };
    let report = safe_write(output, &bytes, &opts, &verify)?;
    let backup = match &report.backup_path {
        Some(path) => format!("backup {}\n", path.display()),
        None => String::new(),
    };
    say(
        out,
        &format!(
            "encoded {} -> {} ({} bytes)\n{backup}",
            json_path.display(),
            output.display(),
            report.bytes_written
        ),
    )
}

// --- keys ---

/// Reads the candidate list: `(line number, candidate)` for every line that is neither blank nor a
/// `#` comment. Line numbers count every line of the file, so they match an editor's numbering.
fn read_candidates(path: &Path) -> Result<Vec<(usize, String)>> {
    let file = fs::File::open(path).map_err(|e| read_error(path, e))?;
    let mut bytes = Vec::new();
    file.take(MAX_CANDIDATE_FILE_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| read_error(path, e))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_CANDIDATE_FILE_BYTES {
        return Err(Error::new(
            KEYS_BAD_LIST,
            format!(
                "the candidate list \"{}\" is larger than 16 MB. Split it into smaller lists.",
                path.display()
            ),
        ));
    }
    let text = String::from_utf8(bytes).map_err(|e| {
        Error::new(
            KEYS_BAD_LIST,
            format!(
                "the candidate list \"{}\" is not UTF-8 text. Save it as UTF-8 with one candidate per line.",
                path.display()
            ),
        )
        .with_source(e)
    })?;
    let mut list = Vec::new();
    for (index, raw) in text.split('\n').enumerate() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if list.len() >= MAX_CANDIDATES {
            return Err(Error::new(
                KEYS_BAD_LIST,
                format!(
                    "the candidate list \"{}\" has more than {MAX_CANDIDATES} candidates. Split it into smaller lists.",
                    path.display()
                ),
            ));
        }
        list.push((index + 1, line.to_string()));
    }
    Ok(list)
}

/// Tries each candidate on the encrypted file. Prints `found: <candidate> (line <n>)` for the first
/// one that opens it. The candidate is never written anywhere else: no log, recipe or file.
fn keys_try(
    path: &Path,
    candidates: &Path,
    reg: &Registry,
    limits: &Limits,
    out: &mut dyn Write,
) -> Result<()> {
    let list = read_candidates(candidates)?;
    if list.is_empty() {
        return Err(Error::new(
            KEYS_BAD_LIST,
            format!(
                "the candidate list \"{}\" has no candidates. Add one password per line; empty lines and lines starting with # are skipped.",
                candidates.display()
            ),
        ));
    }
    let bytes = read_file(path, limits)?;
    let layer = reg.layer(ES3_AES_LAYER).ok_or_else(|| {
        Error::new(
            codes::CORE_UNKNOWN_STEP,
            format!("no layer is registered as \"{ES3_AES_LAYER}\""),
        )
    })?;
    for (line, candidate) in &list {
        let mut hints = Params::new();
        hints.set("password", candidate.as_str());
        let mut ctx = DecodeCtx::new(limits).with_step(0, hints);
        match layer.decode(&bytes, &mut ctx) {
            Ok(_) => return say(out, &format!("found: {candidate} (line {line})\n")),
            // Any other error is about the file itself and would repeat for every candidate.
            Err(error) if error.code() == ES3_WRONG_PASSWORD => {}
            Err(error) => {
                return Err(
                    error.context(format!("while trying the candidates on {}", path.display()))
                );
            }
        }
    }
    Err(Error::new(
        KEYS_NOT_FOUND,
        format!(
            "none of the {} candidates opened \"{}\". Check the password in the game's Easy Save settings, add it to the list (one per line) and try again.",
            list.len(),
            path.display()
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_verification_mismatch_uses_a_core_code() {
        let verify = verify_equal(b"expected");
        assert!(verify(b"expected").is_ok());
        assert_eq!(
            verify(b"other").err().map(|error| error.code()),
            Some(codes::CORE_EDIT_FAILED)
        );
    }
}
