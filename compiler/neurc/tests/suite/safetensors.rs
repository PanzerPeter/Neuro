// `std::safetensors`: `open` and its header checks, `file.tensor(name)` typed by its context
// through `?`, `??` and an annotated `val ... else`, and `Writer` with `add` and `save`.
//
// Each program reads and writes files in its own temporary directory. The files a test hands a
// program are built here byte by byte, so the format is checked against the specification rather
// than against the library's own writer.

use crate::compile_harness::{CompileTest, assert_check_error};
use std::path::{Path, PathBuf};
use std::process::Command;

/// A `.safetensors` file: the header length, the header as written, and the data.
fn safetensors(header: &str, data: &[u8]) -> Vec<u8> {
    let mut bytes = (header.len() as u64).to_le_bytes().to_vec();
    bytes.extend_from_slice(header.as_bytes());
    bytes.extend_from_slice(data);
    bytes
}

/// `[[1, 2, 3], [4, 5, 6]]` as `F32`, `[7, -8]` as `I64` and `[true, false]` as `BOOL`, with
/// metadata, the way PyTorch's writer lays a file out.
fn sample() -> Vec<u8> {
    let header = r#"{"__metadata__":{"format":"pt"},"w":{"dtype":"F32","shape":[2,3],"data_offsets":[0,24]},"b":{"dtype":"I64","shape":[2],"data_offsets":[24,40]},"m":{"dtype":"BOOL","shape":[2],"data_offsets":[40,42]}}"#;
    let mut data = Vec::new();
    for value in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0] {
        data.extend_from_slice(&value.to_le_bytes());
    }
    for value in [7i64, -8] {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data.extend_from_slice(&[1, 0]);
    safetensors(header, &data)
}

/// Compile `source` with `DIR` replaced by its directory, write `files` there, and run it there.
fn run_with(source: &str, files: &[(&str, Vec<u8>)]) -> (i32, String, CompileTest, PathBuf) {
    let test = CompileTest::new();
    let probe = test.write_source("probe.txt", "");
    let dir = probe
        .parent()
        .expect("the source has a directory")
        .to_path_buf();
    let dir_text = dir.display().to_string().replace('\\', "/");
    for (name, bytes) in files {
        std::fs::write(dir.join(name), bytes).expect("write a test file");
    }
    let exe = test
        .compile(&test.write_source("main.nr", &source.replace("DIR", &dir_text)))
        .unwrap_or_else(|e| panic!("should compile: {e}"));
    let output = Command::new(&exe)
        .current_dir(&dir)
        .output()
        .expect("run the program");
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    (output.status.code().unwrap_or(-1), stdout, test, dir)
}

fn read(dir: &Path, name: &str) -> Vec<u8> {
    std::fs::read(dir.join(name)).expect("the program wrote the file")
}

/// One sum, 21, reached through every form that hands a read its type.
#[test]
fn every_annotated_form_reads_the_same_tensor() {
    let source = r#"
func tail(file: &std::safetensors::File) -> Result<Tensor<f32, [2, 3]>, IoError> {
    file.tensor("w")
}

func bound(file: &std::safetensors::File) -> Result<f32, IoError> {
    val w: Tensor<f32, [2, 3]> = file.tensor("w")?
    Result::Ok(w.sum())
}

func argument(w: Tensor<f32, [2, 3]>) -> f32 {
    w.sum()
}

func passed(file: &std::safetensors::File) -> Result<f32, IoError> {
    Result::Ok(argument(file.tensor("w")?))
}

func otherwise(file: &std::safetensors::File) -> f32 {
    val w: Tensor<f32, [2, 3]> = file.tensor("w") ?? Tensor::zeros()
    w.sum()
}

func guarded(file: &std::safetensors::File) -> f32 {
    val Ok(w): Result<Tensor<f32, [2, 3]>, IoError> = file.tensor("w") else |e| {
        return -1.0
    }
    w.sum()
}

func nested(file: &std::safetensors::File, deep: bool) -> f32 {
    if deep {
        match file.metadata("format") {
            Option::Some(_) => {
                val w: Tensor<f32, [2, 3]> = file.tensor("w") ?? panic("no w")
                w.sum()
            },
            Option::None => -1.0
        }
    } else {
        -1.0
    }
}

func temporary(path: &string) -> Result<f32, IoError> {
    val w: Tensor<f32, [2, 3]> = std::safetensors::open(path)?.tensor("w")?
    Result::Ok(w.sum())
}

func main() -> i32 {
    val file = std::safetensors::open("sample.safetensors") ?? panic("cannot open")
    val sums = [
        (tail(&file) ?? Tensor::zeros()).sum(),
        bound(&file) ?? -1.0,
        passed(&file) ?? -1.0,
        otherwise(&file),
        guarded(&file),
        nested(&file, true),
        temporary("sample.safetensors") ?? -1.0
    ]
    for s in sums {
        print("{s} ")
    }
    println("")
    0
}
"#;
    let (code, stdout, _test, _dir) = run_with(source, &[("sample.safetensors", sample())]);
    assert_eq!(code, 0, "{stdout}");
    assert_eq!(stdout, "21.0 21.0 21.0 21.0 21.0 21.0 21.0 \n");
}

#[test]
fn reads_every_dtype_and_the_metadata() {
    let source = r#"
func main() -> i32 {
    val file = std::safetensors::open("sample.safetensors") ?? panic("cannot open")
    val b: Tensor<i64, [2]> = file.tensor("b") ?? panic("no b")
    val m: Tensor<bool, [2]> = file.tensor("m") ?? panic("no m")
    val format = file.metadata("format") ?? "none"
    val other = file.metadata("other") ?? "none"
    println("{b[0]} {b[1]} {m[0]} {m[1]} {format} {other}")
    0
}
"#;
    let (code, stdout, _test, _dir) = run_with(source, &[("sample.safetensors", sample())]);
    assert_eq!(code, 0, "{stdout}");
    assert_eq!(stdout, "7 -8 true false pt none\n");
}

/// A read whose dtype, shape or name does not match is an `Err` naming the tensor and both
/// layouts, and a `BOOL` byte other than 0 or 1 is refused rather than read.
#[test]
fn a_read_checks_the_name_dtype_shape_and_bool_bytes() {
    let source = r#"
func show(read: Result<Tensor<f32, [3, 2]>, IoError>) {
    match read {
        Result::Ok(_) => println("read"),
        Result::Err(e) => println("{e.kind:?}: {e.message}")
    }
}

func main() -> i32 {
    val file = std::safetensors::open("sample.safetensors") ?? panic("cannot open")
    show(file.tensor("w"))
    show(file.tensor("b"))
    show(file.tensor("nothing"))
    val odd = std::safetensors::open("odd.safetensors") ?? panic("cannot open odd")
    val m: Result<Tensor<bool, [2]>, IoError> = odd.tensor("m")
    match m {
        Result::Ok(_) => println("read"),
        Result::Err(e) => println("{e.kind:?}: {e.message}")
    }
    0
}
"#;
    let odd = safetensors(
        r#"{"m":{"dtype":"BOOL","shape":[2],"data_offsets":[0,2]}}"#,
        &[1, 2],
    );
    let (code, stdout, _test, _dir) = run_with(
        source,
        &[("sample.safetensors", sample()), ("odd.safetensors", odd)],
    );
    assert_eq!(code, 0, "{stdout}");
    assert_eq!(
        stdout,
        "InvalidData: cannot read the tensor `w` from sample.safetensors: it is F32 [2, 3], and the read expects F32 [3, 2]\n\
         InvalidData: cannot read the tensor `b` from sample.safetensors: it is I64 [2], and the read expects F32 [3, 2]\n\
         NotFound: cannot read the tensor `nothing` from sample.safetensors: the file has no tensor of that name\n\
         InvalidData: cannot read the tensor `m` from odd.safetensors: a BOOL element holds 2\n"
    );
}

/// Every header check, each on a file that breaks only it.
#[test]
fn malformed_and_truncated_files_are_invalid_data() {
    let f32_pair = r#"{"w":{"dtype":"F32","shape":[2],"data_offsets":[0,8]}}"#;
    let mut too_long = 1000u64.to_le_bytes().to_vec();
    too_long.extend_from_slice(b"{}");
    let mut over_limit = 200_000_000u64.to_le_bytes().to_vec();
    over_limit.extend_from_slice(b"{}");
    let cases: Vec<(&str, Vec<u8>, &str)> = vec![
        (
            "empty",
            Vec::new(),
            "the file is 0 bytes, too short for the 8-byte header length",
        ),
        (
            "short",
            vec![1, 2, 3],
            "the file is 3 bytes, too short for the 8-byte header length",
        ),
        (
            "long",
            too_long,
            "the header length 1000 runs past the end of the file",
        ),
        (
            "limit",
            over_limit,
            "the header length 200000000 is over the 100 MB limit",
        ),
        (
            "cut",
            safetensors(r#"{"w":"#, b""),
            "the header ends in the middle of its JSON",
        ),
        (
            "array",
            safetensors("[]", b""),
            "expected the header's JSON object at byte 8",
        ),
        (
            "twice",
            safetensors(
                r#"{"w":{"dtype":"U8","shape":[1],"data_offsets":[0,1]},"w":{"dtype":"U8","shape":[1],"data_offsets":[1,2]}}"#,
                &[0, 0],
            ),
            "the tensor `w` appears twice",
        ),
        (
            "dtype",
            safetensors(
                r#"{"w":{"dtype":"F8_E4M3","shape":[2],"data_offsets":[0,2]}}"#,
                &[0, 0],
            ),
            "the tensor `w` has the dtype `F8_E4M3`, which no Neuro tensor holds",
        ),
        (
            "size",
            safetensors(
                r#"{"w":{"dtype":"F32","shape":[3],"data_offsets":[0,8]}}"#,
                &[0; 8],
            ),
            "the tensor `w` is F32 [3], 12 bytes, but its `data_offsets` span 8",
        ),
        (
            "truncated",
            safetensors(f32_pair, &[0; 4]),
            "the tensor `w` ends at byte 8 of 4 bytes of data",
        ),
        (
            "tail",
            safetensors(f32_pair, &[0; 9]),
            "no tensor covers bytes 8 to 9 of the data",
        ),
        (
            "gap",
            safetensors(
                r#"{"a":{"dtype":"U8","shape":[1],"data_offsets":[0,1]},"b":{"dtype":"U8","shape":[1],"data_offsets":[2,3]}}"#,
                &[0; 3],
            ),
            "no tensor covers bytes 1 to 2 of the data",
        ),
        (
            "overlap",
            safetensors(
                r#"{"a":{"dtype":"U8","shape":[2],"data_offsets":[0,2]},"b":{"dtype":"U8","shape":[2],"data_offsets":[1,3]}}"#,
                &[0; 3],
            ),
            "the tensor `b` overlaps another at byte 1 of the data",
        ),
        (
            "key",
            safetensors(
                r#"{"w":{"dtype":"U8","shape":[1],"data_offsets":[0,1],"x":1}}"#,
                &[0],
            ),
            "the tensor `w` has a repeated or unknown key `x`",
        ),
        (
            "missing",
            safetensors(r#"{"w":{"dtype":"U8","shape":[1]}}"#, &[0]),
            "the tensor `w` needs `dtype`, `shape` and `data_offsets`",
        ),
        (
            "negative",
            safetensors(
                r#"{"w":{"dtype":"U8","shape":[-1],"data_offsets":[0,1]}}"#,
                &[0],
            ),
            "expected a non-negative integer at byte 36",
        ),
        (
            "huge",
            safetensors(
                r#"{"w":{"dtype":"F64","shape":[4294967296,4294967296],"data_offsets":[0,8]}}"#,
                &[0; 8],
            ),
            "the tensor `w` is too large to hold",
        ),
        (
            "metadata",
            safetensors(r#"{"__metadata__":{"a":1}}"#, b""),
            "the metadata value of `a` is not a string",
        ),
        (
            "surrogate",
            safetensors(
                r#"{"\ud83d":{"dtype":"U8","shape":[1],"data_offsets":[0,1]}}"#,
                &[0],
            ),
            "a high surrogate escape without its low half at byte 10",
        ),
        (
            "utf8",
            safetensors(
                "{\"\u{0}\":{\"dtype\":\"U8\",\"shape\":[1],\"data_offsets\":[0,1]}}",
                &[0],
            ),
            "a control byte inside a string at byte 10",
        ),
        (
            "trailing",
            safetensors(&format!("{f32_pair} x"), &[0; 8]),
            "unexpected bytes after the header at byte 63",
        ),
    ];
    let source = r#"
func main() -> i32 {
    val args = std::env::args()
    mut index: u64 = 1
    while index < args.len() {
        match std::safetensors::open(&args[index]) {
            Result::Ok(_) => println("opened"),
            Result::Err(e) => println("{e.kind:?}: {e.message}")
        }
        index = index + 1
    }
    0
}
"#;
    let files: Vec<(String, Vec<u8>)> = cases
        .iter()
        .map(|(name, bytes, _)| (format!("{name}.st"), bytes.clone()))
        .collect();
    let named: Vec<(&str, Vec<u8>)> = files
        .iter()
        .map(|(name, bytes)| (name.as_str(), bytes.clone()))
        .collect();
    let (_, _, test, dir) = run_with(source, &named);
    let exe = dir.join(if cfg!(target_os = "windows") {
        "main.exe"
    } else {
        "main"
    });
    let output = Command::new(&exe)
        .args(files.iter().map(|(name, _)| name))
        .current_dir(&dir)
        .output()
        .expect("run the program");
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    let expected: String = cases
        .iter()
        .map(|(name, _, problem)| {
            format!("InvalidData: cannot open {name}.st as safetensors: {problem}\n")
        })
        .collect();
    assert_eq!(stdout, expected);
    drop(test);
}

/// A file that parses with escapes and padding opens: `\u` escapes and a surrogate pair decode,
/// whitespace and trailing spaces are allowed, and a zero-size tensor sits between two others.
#[test]
fn escapes_padding_and_empty_tensors_open() {
    let header = "{ \"\\u00e9\\ud83d\\ude00\" : {\"dtype\":\"U8\", \"shape\":[1], \"data_offsets\":[0,1]},\n\"z\":{\"dtype\":\"F32\",\"shape\":[0,4],\"data_offsets\":[1,1]}}   ";
    let source = r#"
func main() -> i32 {
    val file = std::safetensors::open("e.safetensors") ?? panic("cannot open")
    val name: Tensor<u8, [1]> = file.tensor("é😀") ?? panic("no escaped name")
    val z: Tensor<f32, [0, 4]> = file.tensor("z") ?? panic("no z")
    name[0] as i32
}
"#;
    let (code, stdout, _test, _dir) =
        run_with(source, &[("e.safetensors", safetensors(header, &[9]))]);
    assert_eq!(code, 9, "{stdout}");
}

/// A writer's file, byte for byte: metadata first, tensors in the order first added with a
/// repeated name replacing the earlier tensor, the header padded with spaces to 8 bytes.
#[test]
fn a_writer_saves_the_format_byte_for_byte() {
    let source = r#"
func main() -> i32 {
    val w: Tensor<f32, [2]> = [1.5, -2.0]
    val first: Tensor<u8, [1]> = [9]
    val again: Tensor<u8, [2]> = [3, 4]
    mut out = std::safetensors::Writer::new()
    out.add("n\"ame", &first)
    out.add("w", &w)
    out.add("n\"ame", &again)
    out.add_metadata("k", "v")
    out.save("out.safetensors") ?? panic("cannot save")
    0
}
"#;
    let (code, stdout, _test, dir) = run_with(source, &[]);
    assert_eq!(code, 0, "{stdout}");
    let header = r#"{"__metadata__":{"k":"v"},"n\"ame":{"dtype":"U8","shape":[2],"data_offsets":[0,2]},"w":{"dtype":"F32","shape":[2],"data_offsets":[2,10]}}"#;
    let padded = format!("{header}{}", " ".repeat((8 - header.len() % 8) % 8));
    let mut data = vec![3u8, 4];
    data.extend_from_slice(&1.5f32.to_le_bytes());
    data.extend_from_slice(&(-2.0f32).to_le_bytes());
    assert_eq!(read(&dir, "out.safetensors"), safetensors(&padded, &data));
}

/// Every element type the language has goes out and comes back with its bits unchanged.
#[test]
fn every_element_type_round_trips() {
    let types = [
        ("bool", "[true, false]", "BOOL"),
        ("u8", "[0, 255]", "U8"),
        ("i8", "[-128, 127]", "I8"),
        ("u16", "[0, 65535]", "U16"),
        ("i16", "[-32768, 32767]", "I16"),
        ("f16", "[1.5, -0.0]", "F16"),
        ("bf16", "[1.5, -3.25]", "BF16"),
        ("u32", "[0, 4294967295]", "U32"),
        ("i32", "[-2147483648, 2147483647]", "I32"),
        ("f32", "[3.4e38, -1.5]", "F32"),
        ("u64", "[0, 18446744073709551615]", "U64"),
        ("i64", "[-9223372036854775808, 9223372036854775807]", "I64"),
        ("f64", "[3.141592653589793, -0.0]", "F64"),
    ];
    let mut source = String::from(
        "func copy() -> Result<(), IoError> {\n    mut out = std::safetensors::Writer::new()\n",
    );
    for (ty, values, _) in types {
        source += &format!(
            "    val t_{ty}: Tensor<{ty}, [2]> = {values}\n    out.add(\"{ty}\", &t_{ty})\n"
        );
    }
    source += "    out.save(\"first.safetensors\")?\n    val file = std::safetensors::open(\"first.safetensors\")?\n    mut again = std::safetensors::Writer::new()\n";
    for (ty, _, _) in types {
        source += &format!(
            "    val r_{ty}: Tensor<{ty}, [2]> = file.tensor(\"{ty}\")?\n    again.add(\"{ty}\", &r_{ty})\n"
        );
    }
    source += "    again.save(\"second.safetensors\")\n}\n\nfunc main() -> i32 {\n    copy() ?? panic(\"copy failed\")\n    0\n}\n";
    let (code, stdout, _test, dir) = run_with(&source, &[]);
    assert_eq!(code, 0, "{stdout}");
    let first = read(&dir, "first.safetensors");
    assert_eq!(first, read(&dir, "second.safetensors"));
    let length = u64::from_le_bytes(first[..8].try_into().expect("8 bytes")) as usize;
    let header = std::str::from_utf8(&first[8..8 + length]).expect("UTF-8 header");
    for (ty, _, dtype) in types {
        assert!(
            header.contains(&format!("\"{ty}\":{{\"dtype\":\"{dtype}\",\"shape\":[2]")),
            "{ty} in {header}"
        );
    }
    let data = &first[8 + length..];
    assert_eq!(&data[..2], &[1, 0], "bool");
    assert_eq!(&data[2..4], &[0, 255], "u8");
    let f16_bits = [0x3e00u16.to_le_bytes(), 0x8000u16.to_le_bytes()].concat();
    assert_eq!(&data[14..18], f16_bits.as_slice(), "f16");
}

/// A device tensor is read back before it is written, so the file holds its values.
#[cfg(unix)]
#[test]
fn a_device_tensor_is_read_back_when_added() {
    let source = r#"
func main() -> i32 {
    val w: Tensor<f32, [2]> = [1.5, -2.0]
    val g = w.to(Device::GPU(0))
    mut out = std::safetensors::Writer::new()
    out.add("g", &g)
    out.save("gpu.safetensors") ?? panic("cannot save")
    0
}
"#;
    let test = CompileTest::new();
    let path = test.write_source("main.nr", source);
    let dir = path.parent().expect("a directory").to_path_buf();
    let exe = test.compile(&path).expect("compiles");
    let output = Command::new(&exe)
        .current_dir(&dir)
        .output()
        .expect("run the program");
    if String::from_utf8_lossy(&output.stderr).contains("needs an NVIDIA GPU") {
        return;
    }
    assert_eq!(output.status.code(), Some(0));
    let file = read(&dir, "gpu.safetensors");
    let data = &file[file.len() - 8..];
    assert_eq!(
        data,
        [1.5f32.to_le_bytes(), (-2.0f32).to_le_bytes()].concat()
    );
}

/// Reads, writes and every failure path, many times over, leave nothing behind.
#[test]
fn reads_and_writes_release_everything() {
    let test = CompileTest::new();
    let probe = test.write_source("probe.txt", "");
    let dir = probe.parent().expect("a directory").to_path_buf();
    std::fs::write(dir.join("sample.safetensors"), sample()).expect("write the sample");
    let dir_text = dir.display().to_string().replace('\\', "/");
    let source = r#"
func round(path: &string, out_path: &string) -> Result<f32, IoError> {
    val file = std::safetensors::open(path)?
    val w: Tensor<f32, [2, 3]> = file.tensor("w")?
    val Ok(m): Result<Tensor<bool, [2]>, IoError> = file.tensor("m") else |e| {
        return Result::Err(e)
    }
    val wrong: Result<Tensor<f32, [6]>, IoError> = file.tensor("w")
    val missing: Result<Tensor<f32, [6]>, IoError> = file.tensor("none")
    mut out = std::safetensors::Writer::new()
    out.add("w", &w)
    out.add("w", &w)
    out.add("m", &m)
    out.add_metadata("k", "v")
    out.add_metadata("k", "w")
    out.save(out_path)?
    val again = std::safetensors::open(out_path)?
    val back: Tensor<f32, [2, 3]> = again.tensor("w")?
    val k = again.metadata("k") ?? "none"
    Result::Ok(back.sum())
}

func main() -> i32 {
    mut total = 0.0f32
    mut k = 0
    while k < 20 {
        total += round("DIR/sample.safetensors", "DIR/out.safetensors") ?? -100.0
        total += round("DIR/none.safetensors", "DIR/out.safetensors") ?? 1.0
        k += 1
    }
    total as i32 - 400
}
"#
    .replace("DIR", &dir_text);
    assert_eq!(
        test.run_leak_checked("leaks.nr", &source),
        20 * 21 + 20 - 400
    );
}

#[test]
fn a_read_needs_a_tensor_type_from_its_context() {
    assert_check_error(
        "untyped.nr",
        r#"
func main() -> i32 {
    val file = std::safetensors::open("x") ?? panic("no")
    val w = file.tensor("w")
    0
}
"#,
        "the tensor type `file.tensor(...)` reads cannot be inferred here",
    );
    assert_check_error(
        "unwrapped.nr",
        r#"
func main() -> i32 {
    val file = std::safetensors::open("x") ?? panic("no")
    val w: Tensor<f32, [2]> = file.tensor("w")
    0
}
"#,
        "expected Tensor<f32, [2]>, found Result<Tensor<f32, [2]>, IoError>",
    );
    assert_check_error(
        "dynamic.nr",
        r#"
func main() -> i32 {
    val file = std::safetensors::open("x") ?? panic("no")
    val w: Tensor<f32, [?, 2]> = file.tensor("w") ?? panic("no w")
    0
}
"#,
        "a safetensors read",
    );
}

#[test]
fn add_takes_a_borrowed_tensor_and_a_mutable_writer() {
    assert_check_error(
        "value.nr",
        r#"
func main() -> i32 {
    mut out = std::safetensors::Writer::new()
    out.add("w", 5)
    0
}
"#,
        "`add` takes a borrowed tensor, `&t`; found i32",
    );
    assert_check_error(
        "frozen.nr",
        r#"
func main() -> i32 {
    val w: Tensor<f32, [2]> = [1.0, 2.0]
    val out = std::safetensors::Writer::new()
    out.add("w", &w)
    0
}
"#,
        "declare it with `mut`",
    );
    assert_check_error(
        "arity.nr",
        r#"
func main() -> i32 {
    val w: Tensor<f32, [2]> = [1.0, 2.0]
    mut out = std::safetensors::Writer::new()
    out.add(&w)
    0
}
"#,
        "expected 2",
    );
}

/// A tensor cannot take the name the format keeps for metadata.
#[test]
fn saving_a_tensor_named_metadata_fails() {
    let source = r#"
func main() -> i32 {
    val w: Tensor<f32, [1]> = [1.0]
    mut out = std::safetensors::Writer::new()
    out.add("__metadata__", &w)
    match out.save("bad.safetensors") {
        Result::Ok(_) => println("saved"),
        Result::Err(e) => println("{e.kind:?}: {e.message}")
    }
    0
}
"#;
    let (code, stdout, _test, _dir) = run_with(source, &[]);
    assert_eq!(code, 0, "{stdout}");
    assert_eq!(
        stdout,
        "InvalidData: cannot write bad.safetensors: a tensor cannot be named `__metadata__`\n"
    );
}
