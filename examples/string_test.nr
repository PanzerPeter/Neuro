// NEURO Programming Language - String Type Test
// Phase 1: Basic string implementation

// Simple string return
func get_greeting() -> string {
    return "Hello, NEURO!"
}

// String parameter and return
func echo_message(msg: string) -> string {
    return msg
}

// String variable declaration
func format_message() -> string {
    val prefix: string = "Message: "
    val content: string = "String types working!"
    return content
}

// String with escape sequences
func escaped_string() -> string {
    return "Line 1\nLine 2\tTabbed\nLine 3"
}

// Empty string
func empty_string() -> string {
    return ""
}

// String with Unicode escape
func unicode_string() -> string {
    return "Unicode: \u{1F44D}"
}

// Multiple string operations
func complex_example(input: string) -> string {
    val msg1: string = get_greeting()
    val msg2: string = echo_message(input)
    val msg3: string = "Done"
    return msg3
}

// Implicit return with string
func implicit_return() -> string {
    "Implicit string return"
}

// Main function demonstrating strings
func main() -> i32 {
    val greeting: string = get_greeting()
    val message: string = format_message()
    val echoed: string = echo_message("Test echo")
    val escaped: string = escaped_string()
    val empty: string = empty_string()
    val unicode: string = unicode_string()
    val result: string = complex_example("Complex test")
    val implicit: string = implicit_return()

    return 0
}
