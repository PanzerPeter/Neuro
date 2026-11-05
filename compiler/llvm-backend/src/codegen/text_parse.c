// Text to number: the runtime behind `string.parse::<T>()`.
//
// Each entry reads `len` bytes, writes the value's bits to `out` on success, and returns
// a status. The backend turns a nonzero status into the matching `ParseError` variant, so
// the numbering below is shared with `expressions/string_parse.rs`.
//
// The float path does not call `strtod`. Rounding a decimal to `f64` and then to `f16` or
// `bf16` rounds twice, which is wrong for text just past a midpoint of the narrow format,
// and no libc parses to those formats. One exact path serves all four formats instead,
// and gives the same bits on every platform, in any locale.
//
// The value is held as an exact fraction of big integers, and its significand is produced
// bit by bit with a remainder, so rounding sees everything the text said.
//
// ponytail: bitwise division costs a few hundred nanoseconds on short text. An Eisel-Lemire
// fast path is the upgrade if parsing ever shows up in a profile.
//
// Verified against glibc `strtod` / `strtof` over random, midpoint and subnormal text, and
// against exact rational arithmetic for `f16` / `bf16`, with zero mismatches.
//
// Regenerate text_parse.ll with tools/regen_gpu_runtime.sh.

#include <stdint.h>

enum { PARSE_OK, PARSE_EMPTY, PARSE_INVALID, PARSE_OUT_OF_RANGE };

// Every midpoint between adjacent `f64` values has at most 767 significant digits, so
// digits past this cap can only say which side of a midpoint the text falls on, and a
// sticky bit records that.
#define MAX_DIGITS 800
// Text whose decimal point sits past these positions is out of every format's range: the
// largest `f64` is below 10^309, and half its smallest subnormal is above 10^-325.
#define MAX_POINT 310
#define MIN_POINT (-330)
// The largest operand is `MAX_DIGITS` digits or 5^(MAX_DIGITS - MIN_POINT), about 2660
// bits with the normalizing shift.
#define LIMBS 96
#define LIMB_BITS 32
// 5^13, the largest power of five in one limb.
#define FIVE_POW_CHUNK 1220703125u
#define FIVE_CHUNK_EXPONENT 13
#define EXPONENT_CEILING 100000000000LL

typedef struct {
    uint32_t count;
    uint32_t limbs[LIMBS];
} Big;

static void big_set(Big *b, uint32_t value) {
    b->count = value != 0;
    b->limbs[0] = value;
}

static void big_mul_add(Big *b, uint32_t factor, uint32_t addend) {
    uint64_t carry = addend;
    for (uint32_t i = 0; i < b->count; i++) {
        uint64_t product = (uint64_t)b->limbs[i] * factor + carry;
        b->limbs[i] = (uint32_t)product;
        carry = product >> LIMB_BITS;
    }
    if (carry != 0) {
        b->limbs[b->count++] = (uint32_t)carry;
    }
}

static void big_mul_pow5(Big *b, uint32_t exponent) {
    for (; exponent >= FIVE_CHUNK_EXPONENT; exponent -= FIVE_CHUNK_EXPONENT) {
        big_mul_add(b, FIVE_POW_CHUNK, 0);
    }
    for (; exponent > 0; exponent--) {
        big_mul_add(b, 5, 0);
    }
}

static int64_t big_bits(const Big *b) {
    if (b->count == 0) {
        return 0;
    }
    return (int64_t)(b->count - 1) * LIMB_BITS + LIMB_BITS -
           __builtin_clz(b->limbs[b->count - 1]);
}

static void big_shift_left(Big *b, uint32_t bits) {
    uint32_t limbs = bits / LIMB_BITS;
    uint32_t rest = bits % LIMB_BITS;
    if (b->count == 0) {
        return;
    }
    uint32_t top = b->count + limbs;
    if (rest != 0) {
        b->limbs[top] = b->limbs[b->count - 1] >> (LIMB_BITS - rest);
    } else {
        b->limbs[top] = 0;
    }
    for (uint32_t i = b->count - 1; i > 0; i--) {
        uint32_t low = rest != 0 ? b->limbs[i - 1] >> (LIMB_BITS - rest) : 0;
        b->limbs[i + limbs] = (b->limbs[i] << rest) | low;
    }
    b->limbs[limbs] = b->limbs[0] << rest;
    for (uint32_t i = 0; i < limbs; i++) {
        b->limbs[i] = 0;
    }
    b->count = b->limbs[top] != 0 ? top + 1 : top;
}

static int big_less(const Big *a, const Big *b) {
    if (a->count != b->count) {
        return a->count < b->count;
    }
    for (uint32_t i = a->count; i > 0; i--) {
        if (a->limbs[i - 1] != b->limbs[i - 1]) {
            return a->limbs[i - 1] < b->limbs[i - 1];
        }
    }
    return 0;
}

// `a -= b`, where `b <= a`.
static void big_sub(Big *a, const Big *b) {
    uint64_t borrow = 0;
    for (uint32_t i = 0; i < a->count; i++) {
        uint64_t subtrahend = (i < b->count ? b->limbs[i] : 0) + borrow;
        uint64_t limb = a->limbs[i];
        borrow = limb < subtrahend;
        a->limbs[i] = (uint32_t)(limb - subtrahend);
    }
    while (a->count > 0 && a->limbs[a->count - 1] == 0) {
        a->count--;
    }
}

static int is_digit(uint8_t c) {
    return c >= '0' && c <= '9';
}

// `sign` is 1 for a leading `-`. Returns the offset of the first byte after the sign.
static uint64_t read_sign(const uint8_t *text, uint64_t len, int *sign) {
    *sign = len > 0 && text[0] == '-';
    return len > 0 && (text[0] == '-' || text[0] == '+');
}

uint32_t __neuro_parse_int(const uint8_t *text, uint64_t len, uint64_t max_positive,
                           uint64_t max_negative, uint64_t *out) {
    if (len == 0) {
        return PARSE_EMPTY;
    }
    int negative;
    uint64_t i = read_sign(text, len, &negative);
    if (i == len) {
        return PARSE_INVALID;
    }
    uint64_t value = 0;
    int overflowed = 0;
    // A bad byte anywhere outranks a value too large, so the scan runs to the end.
    for (; i < len; i++) {
        if (!is_digit(text[i])) {
            return PARSE_INVALID;
        }
        overflowed |= __builtin_mul_overflow(value, 10, &value);
        overflowed |= __builtin_add_overflow(value, (uint64_t)(text[i] - '0'), &value);
    }
    if (overflowed || value > (negative ? max_negative : max_positive)) {
        return PARSE_OUT_OF_RANGE;
    }
    *out = negative ? 0 - value : value;
    return PARSE_OK;
}

static int matches_ignoring_case(const uint8_t *text, uint64_t len, const char *word) {
    uint64_t i = 0;
    for (; i < len && word[i] != 0; i++) {
        if ((text[i] | 0x20) != (uint8_t)word[i]) {
            return 0;
        }
    }
    return i == len && word[i] == 0;
}

uint32_t __neuro_parse_bool(const uint8_t *text, uint64_t len, uint64_t *out) {
    if (len == 0) {
        return PARSE_EMPTY;
    }
    if (len == 4 && text[0] == 't' && text[1] == 'r' && text[2] == 'u' && text[3] == 'e') {
        *out = 1;
        return PARSE_OK;
    }
    if (len == 5 && text[0] == 'f' && text[1] == 'a' && text[2] == 'l' && text[3] == 's' &&
        text[4] == 'e') {
        *out = 0;
        return PARSE_OK;
    }
    return PARSE_INVALID;
}

// The nearest value of the format to `numerator / denominator * 2^exponent`, ties to even.
// `sticky` says the true value is a little above that fraction.
static uint64_t round_fraction(Big *numerator, Big *denominator, int64_t exponent, int sticky,
                               uint32_t fraction_bits, uint32_t exponent_bits) {
    int64_t shift = big_bits(denominator) - big_bits(numerator);
    if (shift > 0) {
        big_shift_left(numerator, (uint32_t)shift);
    } else {
        big_shift_left(denominator, (uint32_t)-shift);
    }
    if (big_less(numerator, denominator)) {
        big_shift_left(numerator, 1);
        shift++;
    }
    // Now 1 <= numerator / denominator < 2, so this is the value's binary exponent.
    int64_t binary_exponent = exponent - shift;

    // Two bits past the significand leave a round bit even for the widest format.
    int32_t precision = (int32_t)fraction_bits + 1;
    int32_t produced = precision + 2;
    uint64_t quotient = 0;
    for (int32_t i = 0; i < produced; i++) {
        quotient <<= 1;
        if (!big_less(numerator, denominator)) {
            big_sub(numerator, denominator);
            quotient |= 1;
        }
        big_shift_left(numerator, 1);
    }
    sticky |= numerator->count != 0;

    int64_t bias = ((int64_t)1 << (exponent_bits - 1)) - 1;
    int64_t min_exponent = 1 - bias;
    uint64_t infinity = (((uint64_t)1 << exponent_bits) - 1) << fraction_bits;

    // A subnormal keeps fewer bits, one fewer for each step below the smallest normal.
    int64_t kept = precision;
    if (binary_exponent < min_exponent) {
        kept -= min_exponent - binary_exponent;
        if (kept < 0) {
            return 0;
        }
    }
    uint32_t dropped = (uint32_t)(produced - kept);
    uint64_t significand = quotient >> dropped;
    uint64_t remainder = quotient & (((uint64_t)1 << dropped) - 1);
    uint64_t half = (uint64_t)1 << (dropped - 1);
    if (remainder > half || (remainder == half && (sticky || (significand & 1)))) {
        significand++;
    }

    // A subnormal that rounds up into the smallest normal needs nothing more: its carry
    // lands in the exponent field.
    if (binary_exponent < min_exponent) {
        return significand;
    }
    if (significand >> precision) {
        significand >>= 1;
        binary_exponent++;
    }
    if (binary_exponent > bias) {
        return infinity;
    }
    uint64_t fraction = significand & (((uint64_t)1 << fraction_bits) - 1);
    return ((uint64_t)(binary_exponent + bias) << fraction_bits) | fraction;
}

uint32_t __neuro_parse_float(const uint8_t *text, uint64_t len, uint32_t fraction_bits,
                             uint32_t exponent_bits, uint64_t *out) {
    if (len == 0) {
        return PARSE_EMPTY;
    }
    int negative;
    uint64_t i = read_sign(text, len, &negative);
    uint64_t sign = (uint64_t)negative << (fraction_bits + exponent_bits);
    uint64_t infinity = (((uint64_t)1 << exponent_bits) - 1) << fraction_bits;

    if (matches_ignoring_case(text + i, len - i, "inf") ||
        matches_ignoring_case(text + i, len - i, "infinity")) {
        *out = sign | infinity;
        return PARSE_OK;
    }
    if (matches_ignoring_case(text + i, len - i, "nan")) {
        *out = sign | infinity | ((uint64_t)1 << (fraction_bits - 1));
        return PARSE_OK;
    }

    Big numerator;
    big_set(&numerator, 0);
    uint32_t significant = 0;
    int64_t decimal_exponent = 0;
    int sticky = 0;
    int any_digit = 0;
    int past_point = 0;
    for (; i < len; i++) {
        uint8_t c = text[i];
        if (c == '.' && !past_point) {
            past_point = 1;
            continue;
        }
        if (!is_digit(c)) {
            break;
        }
        any_digit = 1;
        if (significant == 0 && c == '0') {
            decimal_exponent -= past_point;
        } else if (significant < MAX_DIGITS) {
            big_mul_add(&numerator, 10, (uint32_t)(c - '0'));
            significant++;
            decimal_exponent -= past_point;
        } else {
            sticky |= c != '0';
            decimal_exponent += !past_point;
        }
    }
    if (!any_digit) {
        return PARSE_INVALID;
    }

    if (i < len && (text[i] == 'e' || text[i] == 'E')) {
        int exponent_negative;
        i++;
        i += read_sign(text + i, len - i, &exponent_negative);
        if (i == len) {
            return PARSE_INVALID;
        }
        int64_t written = 0;
        for (; i < len && is_digit(text[i]); i++) {
            if (written < EXPONENT_CEILING) {
                written = written * 10 + (text[i] - '0');
            }
        }
        decimal_exponent += exponent_negative ? -written : written;
    }
    if (i != len) {
        return PARSE_INVALID;
    }

    int64_t point = (int64_t)significant + decimal_exponent;
    if (significant == 0 || point < MIN_POINT) {
        *out = sign;
        return PARSE_OK;
    }
    if (point > MAX_POINT) {
        *out = sign | infinity;
        return PARSE_OK;
    }

    // 10^e is 5^e * 2^e, so the fives go into the fraction and the twos into its exponent.
    Big denominator;
    big_set(&denominator, 1);
    if (decimal_exponent > 0) {
        big_mul_pow5(&numerator, (uint32_t)decimal_exponent);
    } else {
        big_mul_pow5(&denominator, (uint32_t)-decimal_exponent);
    }
    *out = sign | round_fraction(&numerator, &denominator, decimal_exponent, sticky, fraction_bits,
                                 exponent_bits);
    return PARSE_OK;
}
