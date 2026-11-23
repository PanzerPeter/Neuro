// `std::safetensors`: the weights format PyTorch, JAX and the Hugging Face tools read and write.
//
// A file is an 8-byte little-endian header length, a JSON header naming each tensor's dtype,
// shape and byte range, then the tensors' bytes, little-endian and packed with no padding between
// them. `open` checks the whole header before believing any of it.
//
// `file.tensor(name)` and `out.add(name, &tensor)` are compiler-known: a tensor's element type
// and shape are not yet a type parameter a library function can take, so the compiler reads both
// off the call. A read calls `locate` below with the dtype and the expected shape spelled as text,
// `"F32"` and `"[?, 3]"`, and sizes the tensor from the stored extents it hands back. A write
// calls `append` with the dtype and the tensor's bytes and extents. The compiler copies the bytes
// into and out of the tensor itself.

// The header length, which comes first.
const LENGTH_BYTES: u64 = 8

// The largest header `open` believes. A real one is a few kilobytes.
const HEADER_LIMIT: u64 = 100000000

// The largest `u64` is this times ten plus five, which is how an integer is kept from overflowing
// one digit at a time.
const TENTH_OF_MAX: u64 = 1844674407370955161

// A header is padded with spaces to this multiple, so the data that follows starts aligned.
const HEADER_ALIGN: u64 = 8

// One tensor's header entry. `shape` is its canonical text, which is what a read's expected shape
// is compared with; `extents` size the tensor a read builds. `begin` and `end` count from the
// start of the data.
struct Stored {
    name: string,
    dtype: string,
    shape: string,
    extents: Vec<u64>,
    begin: u64,
    end: u64
}

// A file whose header has been read and checked. The bytes are held whole: a read copies a
// tensor's range out of them.
export struct File {
    path: string,
    bytes: Vec<u8>,
    data_start: u64,
    tensors: Vec<Stored>,
    meta: Vec<(string, string)>
}

impl File {
    // The `__metadata__` entry named `key`, which the format keeps as text.
    func metadata(&self, key: &string) -> Option<string> {
        for index in 0..self.meta.len() {
            if &self.meta[index].0 == key {
                return Option::Some(self.meta[index].1.clone())
            }
        }
        Option::None
    }
}

// Reads `path` and checks its header: a length that fits the file and the limit, JSON that
// parses, no repeated name, known dtypes, and byte ranges that match each tensor's dtype and
// shape and cover the data exactly. A failure is `InvalidData` naming the first problem.
export func open(path: &string) -> Result<File, IoError> {
    val bytes = std::fs::read_bytes(path)?
    val header = match read_header(&bytes) {
        Result::Ok(header) => header,
        Result::Err(problem) => {
            return Result::Err(IoError {
                kind: IoErrorKind::InvalidData,
                message: "cannot open {path} as safetensors: {problem}"
            })
        }
    }
    Result::Ok(File {
        path: path.clone(),
        bytes: bytes,
        data_start: header.data_start,
        tensors: header.tensors,
        meta: header.metadata
    })
}

// What a header says, before it is joined to the bytes it describes.
struct Header {
    data_start: u64,
    tensors: Vec<Stored>,
    metadata: Vec<(string, string)>
}

func read_header(bytes: &Vec<u8>) -> Result<Header, string> {
    val size = bytes.len()
    if size < LENGTH_BYTES {
        return Result::Err("the file is {size} bytes, too short for the 8-byte header length")
    }
    mut length: u64 = 0
    for index in 0..LENGTH_BYTES {
        length = length | ((bytes[index] as u64) << (8 * index))
    }
    if length > HEADER_LIMIT {
        return Result::Err("the header length {length} is over the 100 MB limit")
    }
    if length > size - LENGTH_BYTES {
        return Result::Err("the header length {length} runs past the end of the file")
    }
    val data_start = LENGTH_BYTES + length
    mut reader = Reader { bytes: bytes, at: LENGTH_BYTES, end: data_start }
    val header = reader.header(data_start)?
    reader.skip_space()
    if reader.at != data_start {
        return Result::Err("unexpected bytes after the header at byte {reader.at}")
    }
    check_layout(&header.tensors, size - data_start)?
    Result::Ok(header)
}

// Every tensor's range lies inside the data, and together they cover it with no gap and no
// overlap. A zero-size tensor sits between two others, never inside one.
//
// ponytail: an insertion sort over the ranges, quadratic in the tensor count; a real sort if a
// file with tens of thousands of tensors shows it.
func check_layout(tensors: &Vec<Stored>, data_size: u64) -> Result<(), string> {
    mut order: Vec<u64> = Vec::with_capacity(tensors.len())
    for index in 0..tensors.len() {
        if tensors[index].end > data_size {
            return Result::Err("the tensor `{tensors[index].name}` ends at byte {tensors[index].end} of {data_size} bytes of data")
        }
        order.push(index)
        mut slot = order.len() - 1
        while slot > 0 && before(&tensors[index], &tensors[order[slot - 1]]) {
            order[slot] = order[slot - 1]
            slot -= 1
        }
        order[slot] = index
    }
    mut covered: u64 = 0
    for position in 0..order.len() {
        val stored = &tensors[order[position]]
        if stored.begin > covered {
            return Result::Err("no tensor covers bytes {covered} to {stored.begin} of the data")
        }
        if stored.begin < covered {
            return Result::Err("the tensor `{stored.name}` overlaps another at byte {stored.begin} of the data")
        }
        covered = stored.end
    }
    if covered != data_size {
        return Result::Err("no tensor covers bytes {covered} to {data_size} of the data")
    }
    Result::Ok(())
}

func before(left: &Stored, right: &Stored) -> bool {
    left.begin < right.begin || (left.begin == right.begin && left.end < right.end)
}

// The size of one element of `dtype`, or zero for a dtype no Neuro tensor holds.
func dtype_size(dtype: &string) -> u64 {
    if dtype == "BOOL" || dtype == "U8" || dtype == "I8" {
        return 1
    }
    if dtype == "U16" || dtype == "I16" || dtype == "F16" || dtype == "BF16" {
        return 2
    }
    if dtype == "U32" || dtype == "I32" || dtype == "F32" {
        return 4
    }
    if dtype == "U64" || dtype == "I64" || dtype == "F64" {
        return 8
    }
    0
}

// `[2, 3]`, as the compiler writes a tensor type's shape.
func shape_text(shape: &Vec<u64>) -> string {
    mut text = StringBuilder::new()
    text.push_str("[")
    for index in 0..shape.len() {
        if index > 0 {
            text.push_str(", ")
        }
        text.push_str("{shape[index]}")
    }
    text.push_str("]")
    text.to_string()
}

// A cursor over the header's JSON. Every method that fails says what it found and where, as a
// byte offset into the file.
struct Reader {
    bytes: &Vec<u8>,
    at: u64,
    end: u64
}

impl Reader {
    func skip_space(&mut self) {
        while self.at < self.end && is_space(self.bytes[self.at]) {
            self.at += 1
        }
    }

    // The next byte that is not whitespace, left unread.
    func peek(&mut self) -> Result<u8, string> {
        self.skip_space()
        if self.at >= self.end {
            return Result::Err("the header ends in the middle of its JSON")
        }
        Result::Ok(self.bytes[self.at])
    }

    func expect(&mut self, byte: u8, what: string) -> Result<(), string> {
        if self.peek()? != byte {
            return Result::Err("expected {what} at byte {self.at}")
        }
        self.at += 1
        Result::Ok(())
    }

    // After a member or an element: true at the closing `close`, false at a comma.
    func closes(&mut self, close: u8) -> Result<bool, string> {
        val next = self.peek()?
        self.at += 1
        if next == close {
            return Result::Ok(true)
        }
        if next != ',' as u8 {
            return Result::Err("expected a comma at byte {self.at - 1}")
        }
        Result::Ok(false)
    }

    func header(&mut self, data_start: u64) -> Result<Header, string> {
        mut tensors: Vec<Stored> = Vec::new()
        mut metadata: Vec<(string, string)> = Vec::new()
        mut seen_metadata = false
        self.expect('{' as u8, "the header's JSON object")?
        if self.peek()? == '}' as u8 {
            self.at += 1
            return Result::Ok(Header { data_start: data_start, tensors: tensors, metadata: metadata })
        }
        loop {
            val name = self.text()?
            self.expect(':' as u8, "a colon")?
            if name == "__metadata__" {
                if seen_metadata {
                    return Result::Err("`__metadata__` appears twice")
                }
                seen_metadata = true
                metadata = self.metadata()?
            } else {
                for index in 0..tensors.len() {
                    if tensors[index].name == name {
                        return Result::Err("the tensor `{name}` appears twice")
                    }
                }
                tensors.push(self.entry(name)?)
            }
            if self.closes('}' as u8)? {
                break
            }
        }
        Result::Ok(Header { data_start: data_start, tensors: tensors, metadata: metadata })
    }

    func metadata(&mut self) -> Result<Vec<(string, string)>, string> {
        mut pairs: Vec<(string, string)> = Vec::new()
        self.expect('{' as u8, "the `__metadata__` object")?
        if self.peek()? == '}' as u8 {
            self.at += 1
            return Result::Ok(pairs)
        }
        loop {
            val key = self.text()?
            self.expect(':' as u8, "a colon")?
            if self.peek()? != '"' as u8 {
                return Result::Err("the metadata value of `{key}` is not a string")
            }
            val value = self.text()?
            for index in 0..pairs.len() {
                if pairs[index].0 == key {
                    return Result::Err("the metadata key `{key}` appears twice")
                }
            }
            pairs.push((key, value))
            if self.closes('}' as u8)? {
                return Result::Ok(pairs)
            }
        }
    }

    // One tensor's `{"dtype": ..., "shape": [...], "data_offsets": [begin, end]}`.
    func entry(&mut self, name: string) -> Result<Stored, string> {
        mut dtype: string = ""
        mut shape: Vec<u64> = Vec::new()
        mut offsets: Vec<u64> = Vec::new()
        mut has_dtype = false
        mut has_shape = false
        mut has_offsets = false
        self.expect('{' as u8, "the object describing `{name}`")?
        mut done = false
        if self.peek()? == '}' as u8 {
            self.at += 1
            done = true
        }
        while !done {
            val key = self.text()?
            self.expect(':' as u8, "a colon")?
            if key == "dtype" && !has_dtype {
                dtype = self.text()?
                has_dtype = true
            } else if key == "shape" && !has_shape {
                shape = self.integers()?
                has_shape = true
            } else if key == "data_offsets" && !has_offsets {
                offsets = self.integers()?
                has_offsets = true
            } else {
                return Result::Err("the tensor `{name}` has a repeated or unknown key `{key}`")
            }
            done = self.closes('}' as u8)?
        }
        if !has_dtype || !has_shape || !has_offsets {
            return Result::Err("the tensor `{name}` needs `dtype`, `shape` and `data_offsets`")
        }
        val width = dtype_size(&dtype)
        if width == 0 {
            return Result::Err("the tensor `{name}` has the dtype `{dtype}`, which no Neuro tensor holds")
        }
        if offsets.len() != 2 || offsets[0] > offsets[1] {
            return Result::Err("the tensor `{name}`'s `data_offsets` is not a range [begin, end]")
        }
        mut size = width
        for index in 0..shape.len() {
            if shape[index] != 0 && size > (TENTH_OF_MAX * 10 + 5) / shape[index] {
                return Result::Err("the tensor `{name}` is too large to hold")
            }
            size *= shape[index]
        }
        val text = shape_text(&shape)
        if offsets[1] - offsets[0] != size {
            return Result::Err("the tensor `{name}` is {dtype} {text}, {size} bytes, but its `data_offsets` span {offsets[1] - offsets[0]}")
        }
        Result::Ok(Stored { name: name, dtype: dtype, shape: text, extents: shape, begin: offsets[0], end: offsets[1] })
    }

    // `[a, b, ...]` of non-negative integers.
    func integers(&mut self) -> Result<Vec<u64>, string> {
        mut all: Vec<u64> = Vec::new()
        self.expect('[' as u8, "an array of integers")?
        if self.peek()? == ']' as u8 {
            self.at += 1
            return Result::Ok(all)
        }
        loop {
            all.push(self.integer()?)
            if self.closes(']' as u8)? {
                return Result::Ok(all)
            }
        }
    }

    func integer(&mut self) -> Result<u64, string> {
        val start = self.at
        val first = self.peek()?
        if !is_digit(first) {
            return Result::Err("expected a non-negative integer at byte {self.at}")
        }
        mut value: u64 = 0
        while self.at < self.end && is_digit(self.bytes[self.at]) {
            val digit = (self.bytes[self.at] - '0' as u8) as u64
            if value > TENTH_OF_MAX || (value == TENTH_OF_MAX && digit > 5) {
                return Result::Err("the integer at byte {start} does not fit 64 bits")
            }
            value = value * 10 + digit
            self.at += 1
        }
        if first == '0' as u8 && self.at - start > 1 {
            return Result::Err("the integer at byte {start} has a leading zero")
        }
        if self.at < self.end {
            val next = self.bytes[self.at]
            if next == '.' as u8 || next == 'e' as u8 || next == 'E' as u8 {
                return Result::Err("expected a whole number at byte {start}")
            }
        }
        Result::Ok(value)
    }

    // A JSON string, its escapes decoded and its UTF-8 checked.
    //
    // ponytail: one interpolated push per scalar; a byte-range copy if headers with long names
    // ever show up in a profile.
    func text(&mut self) -> Result<string, string> {
        self.expect('"' as u8, "a string")?
        mut out = StringBuilder::new()
        loop {
            if self.at >= self.end {
                return Result::Err("a string runs past the end of the header")
            }
            val byte = self.bytes[self.at]
            if byte == '"' as u8 {
                self.at += 1
                return Result::Ok(out.to_string())
            }
            if byte < 32 {
                return Result::Err("a control byte inside a string at byte {self.at}")
            }
            val scalar = if byte == '\\' as u8 {
                self.escape()?
            } else {
                self.scalar()?
            }
            out.push_str("{scalar}")
        }
    }

    // One UTF-8 scalar, refusing what RFC 3629 rules out: overlong forms, surrogates, and values
    // past U+10FFFF.
    func scalar(&mut self) -> Result<char, string> {
        val lead = self.bytes[self.at] as u32
        if lead < 128 {
            self.at += 1
            return Result::Ok(lead as char)
        }
        mut width: u64 = 0
        mut code: u32 = 0
        mut least: u32 = 0
        if lead >= 194 && lead <= 223 {
            width = 2
            code = lead & 31
            least = 128
        } else if lead >= 224 && lead <= 239 {
            width = 3
            code = lead & 15
            least = 2048
        } else if lead >= 240 && lead <= 244 {
            width = 4
            code = lead & 7
            least = 65536
        } else {
            return Result::Err("invalid UTF-8 at byte {self.at}")
        }
        if width > self.end - self.at {
            return Result::Err("invalid UTF-8 at byte {self.at}")
        }
        for index in 1..width {
            val next = self.bytes[self.at + index] as u32
            if (next & 192) != 128 {
                return Result::Err("invalid UTF-8 at byte {self.at}")
            }
            code = (code << 6) | (next & 63)
        }
        if code < least || code > 1114111 || (code >= 55296 && code <= 57343) {
            return Result::Err("invalid UTF-8 at byte {self.at}")
        }
        self.at += width
        Result::Ok(code as char)
    }

    // The escape after a backslash. A `\u` high surrogate must be followed by its low half.
    func escape(&mut self) -> Result<char, string> {
        val at = self.at
        if self.end - self.at < 2 {
            return Result::Err("a string runs past the end of the header")
        }
        val kind = self.bytes[self.at + 1] as char
        self.at += 2
        match kind {
            '"' => Result::Ok('"'),
            '\\' => Result::Ok('\\'),
            '/' => Result::Ok('/'),
            'b' => Result::Ok(8u32 as char),
            'f' => Result::Ok(12u32 as char),
            'n' => Result::Ok('\n'),
            'r' => Result::Ok('\r'),
            't' => Result::Ok('\t'),
            'u' => {
                val high = self.hex4()?
                if high >= 56320 && high <= 57343 {
                    return Result::Err("a lone low surrogate escape at byte {at}")
                }
                if high < 55296 || high > 56319 {
                    return Result::Ok(high as char)
                }
                if self.end - self.at < 6 || self.bytes[self.at] != '\\' as u8 || self.bytes[self.at + 1] != 'u' as u8 {
                    return Result::Err("a high surrogate escape without its low half at byte {at}")
                }
                self.at += 2
                val low = self.hex4()?
                if low < 56320 || low > 57343 {
                    return Result::Err("a high surrogate escape without its low half at byte {at}")
                }
                Result::Ok((65536 + ((high - 55296) << 10) + (low - 56320)) as char)
            },
            _ => Result::Err("an unknown escape at byte {at}")
        }
    }

    func hex4(&mut self) -> Result<u32, string> {
        if self.end - self.at < 4 {
            return Result::Err("a `\\u` escape runs past the end of the header")
        }
        mut value: u32 = 0
        for index in 0u64..4 {
            val byte = self.bytes[self.at + index] as u32
            mut digit: u32 = 16
            if byte >= 48 && byte <= 57 {
                digit = byte - 48
            } else if byte >= 97 && byte <= 102 {
                digit = byte - 87
            } else if byte >= 65 && byte <= 70 {
                digit = byte - 55
            }
            if digit == 16 {
                return Result::Err("a `\\u` escape with a non-hex digit at byte {self.at + index}")
            }
            value = value * 16 + digit
        }
        self.at += 4
        Result::Ok(value)
    }
}

func is_space(byte: u8) -> bool {
    byte == ' ' as u8 || byte == '\t' as u8 || byte == '\n' as u8 || byte == '\r' as u8
}

func is_digit(byte: u8) -> bool {
    byte >= '0' as u8 && byte <= '9' as u8
}

// Whether the stored shape text `[2, 784]` is the expected `[?, 784]`, a `?` standing for any one
// extent. Both are the canonical text, so they agree byte for byte everywhere else.
func shape_fits(stored: &string, expected: &string) -> bool {
    mut have: Vec<u8> = Vec::new()
    push_utf8(&mut have, stored)
    mut want: Vec<u8> = Vec::new()
    push_utf8(&mut want, expected)
    mut at: u64 = 0
    for index in 0..want.len() {
        if want[index] == '?' as u8 {
            if at >= have.len() || !is_digit(have[at]) {
                return false
            }
            while at < have.len() && is_digit(have[at]) {
                at += 1
            }
        } else {
            if at >= have.len() || have[at] != want[index] {
                return false
            }
            at += 1
        }
    }
    at == have.len()
}

// Called by `file.tensor(name)`: the stored bytes and extents of `name`, after checking its dtype
// and shape against the ones the read was annotated with.
func locate(file: &File, name: &string, dtype: &string, shape: &string) -> Result<(&[u8], &[u64]), IoError> {
    for index in 0..file.tensors.len() {
        val stored = &file.tensors[index]
        if &stored.name == name {
            if &stored.dtype != dtype || !shape_fits(&stored.shape, shape) {
                return Result::Err(IoError {
                    kind: IoErrorKind::InvalidData,
                    message: "cannot read the tensor `{name}` from {file.path}: it is {stored.dtype} {stored.shape}, and the read expects {dtype} {shape}"
                })
            }
            val bytes = file.bytes.slice((file.data_start + stored.begin)..(file.data_start + stored.end))
            if dtype == "BOOL" {
                for at in 0..bytes.len() {
                    if bytes[at] > 1 {
                        return Result::Err(IoError {
                            kind: IoErrorKind::InvalidData,
                            message: "cannot read the tensor `{name}` from {file.path}: a BOOL element holds {bytes[at]}"
                        })
                    }
                }
            }
            val extents = file.tensors[index].extents.slice(0..stored.extents.len())
            return Result::Ok((bytes, extents))
        }
    }
    Result::Err(IoError {
        kind: IoErrorKind::NotFound,
        message: "cannot read the tensor `{name}` from {file.path}: the file has no tensor of that name"
    })
}

struct Pending {
    name: string,
    dtype: string,
    shape: string,
    data: Vec<u8>
}

// Tensors to be saved together. `add` copies a tensor's elements in, so a writer holds no borrow.
export struct Writer {
    tensors: Vec<Pending>,
    metadata: Vec<(string, string)>
}

impl Writer {
    func new() -> Writer {
        Writer { tensors: Vec::new(), metadata: Vec::new() }
    }

    // Sets the `__metadata__` entry `key`, replacing an earlier value.
    func add_metadata(&mut self, key: &string, value: &string) {
        mut index: u64 = 0
        while index < self.metadata.len() && &self.metadata[index].0 != key {
            index += 1
        }
        if index < self.metadata.len() {
            self.metadata[index] = (key.clone(), value.clone())
        } else {
            self.metadata.push((key.clone(), value.clone()))
        }
    }

    // Writes every tensor added, in the order they were first added, creating `path` or
    // replacing what it held.
    //
    // ponytail: the file is assembled one byte push at a time; a byte-append intrinsic if
    // checkpoints get large enough for it to show.
    func save(&self, path: &string) -> Result<(), IoError> {
        mut header = StringBuilder::new()
        header.push_str("\{")
        mut first = true
        if self.metadata.len() > 0 {
            header.push_str("\"__metadata__\":\{")
            for index in 0..self.metadata.len() {
                if index > 0 {
                    header.push_str(",")
                }
                header.push_str(json_text(&self.metadata[index].0))
                header.push_str(":")
                header.push_str(json_text(&self.metadata[index].1))
            }
            header.push_str("\}")
            first = false
        }
        mut offset: u64 = 0
        for index in 0..self.tensors.len() {
            val pending = &self.tensors[index]
            if pending.name == "__metadata__" {
                return Result::Err(IoError {
                    kind: IoErrorKind::InvalidData,
                    message: "cannot write {path}: a tensor cannot be named `__metadata__`"
                })
            }
            if !first {
                header.push_str(",")
            }
            first = false
            val end = offset + pending.data.len()
            header.push_str(json_text(&pending.name))
            header.push_str(":\{\"dtype\":\"{pending.dtype}\",\"shape\":{pending.shape},\"data_offsets\":[{offset},{end}]\}")
            offset = end
        }
        header.push_str("\}")
        val text = header.to_string()
        mut bytes: Vec<u8> = Vec::with_capacity(LENGTH_BYTES + text.len() + HEADER_ALIGN + offset)
        for slot in 0..LENGTH_BYTES {
            bytes.push(0)
        }
        push_utf8(&mut bytes, &text)
        while bytes.len() % HEADER_ALIGN != 0 {
            bytes.push(' ' as u8)
        }
        val length = bytes.len() - LENGTH_BYTES
        for slot in 0..LENGTH_BYTES {
            bytes[slot] = (length.shr(8 * slot) & 255) as u8
        }
        for index in 0..self.tensors.len() {
            val data = &self.tensors[index].data
            for at in 0..data.len() {
                bytes.push(data[at])
            }
        }
        std::fs::write_bytes(path, &bytes)
    }
}

// Called by `out.add(name, &tensor)` with the tensor's bytes and extents already copied out. A name
// added again replaces the earlier tensor, as an entry of a Python dict would.
func append(out: &mut Writer, name: &string, dtype: &string, parts: (Vec<u8>, Vec<u64>)) {
    mut index: u64 = 0
    while index < out.tensors.len() && &out.tensors[index].name != name {
        index += 1
    }
    val (data, extents) = parts
    val pending = Pending { name: name.clone(), dtype: dtype.clone(), shape: shape_text(&extents), data: data }
    if index < out.tensors.len() {
        out.tensors[index] = pending
    } else {
        out.tensors.push(pending)
    }
}

// `text` as a JSON string literal.
func json_text(text: &string) -> string {
    mut out = StringBuilder::new()
    out.push_str("\"")
    for c in text.chars() {
        val code = c as u32
        if c == '"' {
            out.push_str("\\\"")
        } else if c == '\\' {
            out.push_str("\\\\")
        } else if code < 32 {
            out.push_str("\\u00")
            out.push_str("{hex_digit(code / 16)}{hex_digit(code % 16)}")
        } else {
            out.push_str("{c}")
        }
    }
    out.push_str("\"")
    out.to_string()
}

func hex_digit(value: u32) -> char {
    if value < 10 {
        return (48 + value) as char
    }
    (87 + value) as char
}

func push_utf8(bytes: &mut Vec<u8>, text: &string) {
    for c in text.chars() {
        val code = c as u32
        if code < 128 {
            bytes.push(code as u8)
        } else if code < 2048 {
            bytes.push((192 | code.shr(6)) as u8)
            bytes.push((128 | (code & 63)) as u8)
        } else if code < 65536 {
            bytes.push((224 | code.shr(12)) as u8)
            bytes.push((128 | (code.shr(6) & 63)) as u8)
            bytes.push((128 | (code & 63)) as u8)
        } else {
            bytes.push((240 | code.shr(18)) as u8)
            bytes.push((128 | (code.shr(12) & 63)) as u8)
            bytes.push((128 | (code.shr(6) & 63)) as u8)
            bytes.push((128 | (code & 63)) as u8)
        }
    }
}
