// The operating-system runtime behind `std::fs`, `std::env`, `std::io::read_line` and
// `std::time`.
//
// The generated IR is linked into programs for every host, so this file includes no system
// header and names no type whose width differs between systems: sizes are `uint64_t`, and a
// `FILE *` is an opaque pointer. Everything that does differ (where `errno` lives, which clock
// is monotonic) is reached through the function pointers below, which the backend fills in
// for the system the program is built for.
//
// An entry that hands text back writes a buffer from `malloc` to `*out` and its byte count to
// `*out_len`, and returns a status. The program owns the buffer and frees it. The statuses are
// shared with `os_runtime.rs` and with the standard library, which maps 1 to 4 onto
// `IoErrorKind` in declaration order; on those `*out` holds the system's reason as text.
//
// Regenerate os_runtime.ll with tools/regen_gpu_runtime.sh.

#include <stdint.h>

enum {
    STATUS_OK,
    STATUS_NOT_FOUND,
    STATUS_PERMISSION_DENIED,
    STATUS_INVALID_DATA,
    STATUS_OTHER,
    STATUS_NONE,
};

// The errno values the kinds are read from. POSIX leaves the numbers to each system, but
// Linux, macOS and the Windows CRT agree on these three.
#define ERRNO_PERMISSION 1
#define ERRNO_NO_ENTRY 2
#define ERRNO_ACCESS 13

// What a file read asks for at a time, and the start of a read buffer.
#define READ_CHUNK 65536

#define NANOS_PER_SECOND 1000000000

typedef struct file file;

void *malloc(uint64_t size);
void *realloc(void *block, uint64_t size);
void free(void *block);
file *fopen(const char *path, const char *mode);
uint64_t fread(void *buffer, uint64_t size, uint64_t count, file *stream);
uint64_t fwrite(const void *buffer, uint64_t size, uint64_t count, file *stream);
int32_t fclose(file *stream);
int32_t ferror(file *stream);
int32_t remove(const char *path);
char *getenv(const char *name);
char *strerror(int32_t code);
// `ssize_t read(int, void *, size_t)` on POSIX and `int _read(int, void *, unsigned)` on
// Windows: reading the result as 32 bits is right for both, and a 64-bit count is read whole by
// the first and in its low half by the second.
int32_t read(int32_t fd, void *buffer, uint64_t count);

int32_t *(*__neuro_errno_location)(void);
int32_t (*__neuro_clock_gettime)(int32_t clock, int64_t *timespec);
int32_t __neuro_clock_id;
int32_t (*__neuro_query_counter)(int64_t *count);
int32_t (*__neuro_query_frequency)(int64_t *frequency);

// The command line, stored by the program's entry point before anything else runs.
int32_t __neuro_argc;
char **__neuro_argv;

static uint64_t text_length(const char *text) {
    uint64_t length = 0;
    while (text[length] != 0) {
        length++;
    }
    return length;
}

static void copy_bytes(uint8_t *to, const uint8_t *from, uint64_t count) {
    for (uint64_t i = 0; i < count; i++) {
        to[i] = from[i];
    }
}

static int32_t give(uint8_t *bytes, uint64_t length, uint8_t **out, int64_t *out_len) {
    *out = bytes;
    *out_len = (int64_t)length;
    return STATUS_OK;
}

static int32_t give_copy(const char *text, uint8_t **out, int64_t *out_len) {
    uint64_t length = text_length(text);
    uint8_t *copy = malloc(length);
    if (copy == 0 && length > 0) {
        return STATUS_OTHER;
    }
    copy_bytes(copy, (const uint8_t *)text, length);
    return give(copy, length, out, out_len);
}

static int32_t fail(int32_t status, const char *reason, uint8_t **out, int64_t *out_len) {
    give_copy(reason, out, out_len);
    return status;
}

static int32_t fail_errno(uint8_t **out, int64_t *out_len) {
    int32_t code = *__neuro_errno_location();
    int32_t status = STATUS_OTHER;
    if (code == ERRNO_NO_ENTRY) {
        status = STATUS_NOT_FOUND;
    } else if (code == ERRNO_ACCESS || code == ERRNO_PERMISSION) {
        status = STATUS_PERMISSION_DENIED;
    }
    return fail(status, strerror(code), out, out_len);
}

// A NUL-terminated copy of `bytes`, or null when they hold a NUL of their own (no system
// call can be given such a name) or memory ran out.
static char *c_string(const uint8_t *bytes, int64_t length) {
    for (int64_t i = 0; i < length; i++) {
        if (bytes[i] == 0) {
            return 0;
        }
    }
    char *copy = malloc((uint64_t)length + 1);
    if (copy == 0) {
        return 0;
    }
    copy_bytes((uint8_t *)copy, bytes, (uint64_t)length);
    copy[length] = 0;
    return copy;
}

// The byte count of the valid UTF-8 sequence starting at `at`, or 0 when none does: an
// overlong form, a surrogate and anything past U+10FFFF are not valid.
static uint64_t utf8_sequence(const uint8_t *bytes, uint64_t at, uint64_t length) {
    uint8_t lead = bytes[at];
    if (lead < 0x80) {
        return 1;
    }
    uint64_t width;
    uint8_t low = 0x80;
    uint8_t high = 0xBF;
    if (lead >= 0xC2 && lead <= 0xDF) {
        width = 2;
    } else if (lead >= 0xE0 && lead <= 0xEF) {
        width = 3;
        if (lead == 0xE0) {
            low = 0xA0;
        } else if (lead == 0xED) {
            high = 0x9F;
        }
    } else if (lead >= 0xF0 && lead <= 0xF4) {
        width = 4;
        if (lead == 0xF0) {
            low = 0x90;
        } else if (lead == 0xF4) {
            high = 0x8F;
        }
    } else {
        return 0;
    }
    if (length - at < width) {
        return 0;
    }
    // Only the first continuation byte has a narrowed range.
    if (bytes[at + 1] < low || bytes[at + 1] > high) {
        return 0;
    }
    for (uint64_t i = 2; i < width; i++) {
        if (bytes[at + i] < 0x80 || bytes[at + i] > 0xBF) {
            return 0;
        }
    }
    return width;
}

static int32_t is_utf8(const uint8_t *bytes, uint64_t length) {
    uint64_t at = 0;
    while (at < length) {
        uint64_t width = utf8_sequence(bytes, at, length);
        if (width == 0) {
            return 0;
        }
        at += width;
    }
    return 1;
}

// `text` as UTF-8, each byte that begins no valid sequence read as U+FFFD.
static int32_t give_lossy(const char *text, uint8_t **out, int64_t *out_len) {
    const uint8_t *bytes = (const uint8_t *)text;
    uint64_t length = text_length(text);
    // A replacement is three bytes standing for one.
    uint8_t *copy = malloc(length * 3);
    if (copy == 0 && length > 0) {
        return STATUS_OTHER;
    }
    uint64_t written = 0;
    uint64_t at = 0;
    while (at < length) {
        uint64_t width = utf8_sequence(bytes, at, length);
        if (width == 0) {
            copy[written++] = 0xEF;
            copy[written++] = 0xBF;
            copy[written++] = 0xBD;
            at++;
            continue;
        }
        copy_bytes(copy + written, bytes + at, width);
        written += width;
        at += width;
    }
    return give(copy, written, out, out_len);
}

static int32_t read_file(const uint8_t *path, int64_t path_len, int32_t text, uint8_t **out,
                         int64_t *out_len) {
    char *name = c_string(path, path_len);
    if (name == 0) {
        return fail(STATUS_OTHER, "the path holds a NUL byte", out, out_len);
    }
    file *stream = fopen(name, "rb");
    free(name);
    if (stream == 0) {
        return fail_errno(out, out_len);
    }
    // The size is not asked for up front: a pipe or a file the kernel generates reports none.
    uint64_t capacity = READ_CHUNK;
    uint64_t length = 0;
    uint8_t *bytes = malloc(capacity);
    while (bytes != 0) {
        if (length == capacity) {
            capacity *= 2;
            uint8_t *grown = realloc(bytes, capacity);
            if (grown == 0) {
                free(bytes);
                bytes = 0;
                break;
            }
            bytes = grown;
        }
        uint64_t got = fread(bytes + length, 1, capacity - length, stream);
        length += got;
        if (got == 0) {
            break;
        }
    }
    if (bytes == 0) {
        fclose(stream);
        return fail(STATUS_OTHER, "out of memory", out, out_len);
    }
    if (ferror(stream)) {
        int32_t status = fail_errno(out, out_len);
        free(bytes);
        fclose(stream);
        return status;
    }
    fclose(stream);
    if (text && !is_utf8(bytes, length)) {
        free(bytes);
        return fail(STATUS_INVALID_DATA, "the file is not valid UTF-8", out, out_len);
    }
    return give(bytes, length, out, out_len);
}

int32_t __neuro_read_text(const uint8_t *path, int64_t path_len, uint8_t **out,
                          int64_t *out_len) {
    return read_file(path, path_len, 1, out, out_len);
}

int32_t __neuro_read_bytes(const uint8_t *path, int64_t path_len, uint8_t **out,
                           int64_t *out_len) {
    return read_file(path, path_len, 0, out, out_len);
}

int32_t __neuro_write_file(const uint8_t *path, int64_t path_len, const uint8_t *data,
                           int64_t data_len, uint8_t **out, int64_t *out_len) {
    *out = 0;
    *out_len = 0;
    char *name = c_string(path, path_len);
    if (name == 0) {
        return fail(STATUS_OTHER, "the path holds a NUL byte", out, out_len);
    }
    file *stream = fopen(name, "wb");
    free(name);
    if (stream == 0) {
        return fail_errno(out, out_len);
    }
    uint64_t written = fwrite(data, 1, (uint64_t)data_len, stream);
    // A short write or a failed close (the disk filled while the buffer drained) both lose data.
    if (written != (uint64_t)data_len) {
        int32_t status = fail_errno(out, out_len);
        fclose(stream);
        return status;
    }
    if (fclose(stream) != 0) {
        return fail_errno(out, out_len);
    }
    return STATUS_OK;
}

int32_t __neuro_remove_file(const uint8_t *path, int64_t path_len, uint8_t **out,
                            int64_t *out_len) {
    *out = 0;
    *out_len = 0;
    char *name = c_string(path, path_len);
    if (name == 0) {
        return fail(STATUS_OTHER, "the path holds a NUL byte", out, out_len);
    }
    int32_t result = remove(name);
    free(name);
    if (result != 0) {
        return fail_errno(out, out_len);
    }
    return STATUS_OK;
}

int64_t __neuro_arg_count(void) {
    return __neuro_argc;
}

int32_t __neuro_arg(int64_t index, uint8_t **out, int64_t *out_len) {
    if (index < 0 || index >= __neuro_argc) {
        return give_copy("", out, out_len);
    }
    return give_lossy(__neuro_argv[index], out, out_len);
}

// `STATUS_NONE` for a variable that is not set, and for a name no variable can have.
int32_t __neuro_env_var(const uint8_t *name, int64_t name_len, uint8_t **out,
                        int64_t *out_len) {
    *out = 0;
    *out_len = 0;
    for (int64_t i = 0; i < name_len; i++) {
        if (name[i] == '=') {
            return STATUS_NONE;
        }
    }
    char *key = c_string(name, name_len);
    if (key == 0 || name_len == 0) {
        free(key);
        return STATUS_NONE;
    }
    char *value = getenv(key);
    free(key);
    if (value == 0) {
        return STATUS_NONE;
    }
    return give_lossy(value, out, out_len);
}

// What standard input has delivered and no line has taken yet. One process reads one
// standard input, so this is the one cursor over it.
static uint8_t stdin_buffer[READ_CHUNK];
static int64_t stdin_start;
static int64_t stdin_end;
static int32_t stdin_closed;

// `STATUS_NONE` once standard input is exhausted with nothing left over.
int32_t __neuro_read_line(uint8_t **out, int64_t *out_len) {
    *out = 0;
    *out_len = 0;
    uint64_t capacity = 0;
    uint64_t length = 0;
    uint8_t *line = 0;
    int32_t ended = 0;
    while (!ended) {
        if (stdin_start == stdin_end) {
            if (stdin_closed) {
                break;
            }
            int32_t got = read(0, stdin_buffer, READ_CHUNK);
            if (got < 0) {
                free(line);
                return fail_errno(out, out_len);
            }
            if (got == 0) {
                stdin_closed = 1;
                break;
            }
            stdin_start = 0;
            stdin_end = got;
        }
        int64_t stop = stdin_start;
        while (stop < stdin_end && stdin_buffer[stop] != '\n') {
            stop++;
        }
        uint64_t take = (uint64_t)(stop - stdin_start);
        if (length + take > capacity) {
            capacity = (length + take) * 2;
            uint8_t *grown = realloc(line, capacity);
            if (grown == 0) {
                free(line);
                return fail(STATUS_OTHER, "out of memory", out, out_len);
            }
            line = grown;
        }
        copy_bytes(line + length, stdin_buffer + stdin_start, take);
        length += take;
        stdin_start = stop;
        if (stop < stdin_end) {
            stdin_start++;
            ended = 1;
        }
    }
    if (!ended && length == 0) {
        free(line);
        return STATUS_NONE;
    }
    if (length > 0 && line[length - 1] == '\r') {
        length--;
    }
    if (!is_utf8(line, length)) {
        free(line);
        return fail(STATUS_INVALID_DATA, "the line is not valid UTF-8", out, out_len);
    }
    return give(line, length, out, out_len);
}

int64_t __neuro_monotonic_nanos(void) {
    if (__neuro_clock_gettime != 0) {
        int64_t now[2] = {0, 0};
        __neuro_clock_gettime(__neuro_clock_id, now);
        return now[0] * NANOS_PER_SECOND + now[1];
    }
    int64_t count = 0;
    int64_t frequency = 1;
    __neuro_query_counter(&count);
    __neuro_query_frequency(&frequency);
    // Split so the product never overflows: a counter at 10 MHz reaches 2^63 / 10^9 ticks
    // in about fifteen minutes.
    return count / frequency * NANOS_PER_SECOND + count % frequency * NANOS_PER_SECOND / frequency;
}
