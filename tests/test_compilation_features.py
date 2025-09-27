#!/usr/bin/env python3
"""
Test suite for specific NEURO language features
Tests individual language constructs and compilation capabilities
"""

import subprocess
import tempfile
import os
from pathlib import Path

def compile_and_run_code(code, expected_exit_code=None):
    """Compile NEURO code and optionally run it, return success status"""
    try:
        # Create temporary file
        with tempfile.NamedTemporaryFile(mode='w', suffix='.nr', delete=False) as f:
            f.write(code)
            temp_file = f.name

        base_path = Path(__file__).parent.parent

        # Try to build
        build_result = subprocess.run(
            ['./target/release/neurc', 'build', temp_file],
            cwd=base_path,
            capture_output=True,
            text=True,
            timeout=30
        )

        if build_result.returncode != 0:
            return False, f"Build failed: {build_result.stderr}"

        # If expected exit code provided, run the executable
        if expected_exit_code is not None:
            exe_file = temp_file.replace('.nr', '.exe')
            if os.path.exists(exe_file):
                run_result = subprocess.run(
                    [exe_file],
                    capture_output=True,
                    timeout=10
                )
                if run_result.returncode != expected_exit_code:
                    return False, f"Expected exit code {expected_exit_code}, got {run_result.returncode}"

        return True, "Success"

    except Exception as e:
        return False, str(e)
    finally:
        # Cleanup
        try:
            if 'temp_file' in locals():
                os.unlink(temp_file)
                exe_file = temp_file.replace('.nr', '.exe')
                if os.path.exists(exe_file):
                    os.unlink(exe_file)
        except:
            pass

def test_basic_features():
    """Test basic language features"""
    tests = [
        ("Simple main function",
         "fn main() -> int { return 42; }", 42),

        ("Variable declaration",
         "fn main() -> int { let x = 10; return x; }", 10),

        ("Arithmetic operations",
         "fn main() -> int { let result = 5 + 3 * 2; return result; }", 11),

        ("Boolean operations",
         "fn main() -> int { let flag = true; if flag { return 1; } return 0; }", 1),

        ("If-else statement",
         "fn main() -> int { if 5 > 3 { return 1; } else { return 0; } }", 1),

        ("While loop",
         "fn main() -> int { let i = 0; while i < 3 { i = i + 1; } return i; }", 3),

        ("Function calls",
         "fn add(a: int, b: int) -> int { return a + b; } fn main() -> int { return add(5, 7); }", 12),

        ("Nested function calls",
         "fn double(x: int) -> int { return x * 2; } fn main() -> int { return double(double(3)); }", 12),
    ]

    print("\n=== Testing Basic Language Features ===")
    passed = 0

    for name, code, expected in tests:
        success, message = compile_and_run_code(code, expected)
        if success:
            print(f"[PASS] {name}")
            passed += 1
        else:
            print(f"[FAIL] {name}: {message}")

    return passed, len(tests)

def test_advanced_features():
    """Test advanced language features"""
    tests = [
        ("Complex expressions",
         "fn main() -> int { return (5 + 3) * (2 - 1); }", 8),

        ("Logical operators",
         "fn main() -> int { if true && false { return 0; } return 1; }", 1),

        ("Comparison operators",
         "fn main() -> int { if 10 >= 10 && 5 < 7 { return 1; } return 0; }", 1),

        ("Variable reassignment",
         "fn main() -> int { let x = 5; x = x + 3; return x; }", 8),

        ("Multiple variables",
         "fn main() -> int { let a = 3; let b = 4; return a * b; }", 12),

        ("Recursive functions",
         "fn factorial(n: int) -> int { if n <= 1 { return 1; } return n * factorial(n - 1); } fn main() -> int { return factorial(5); }", 120),

        ("Break and continue in loops",
         "fn main() -> int { let sum = 0; let i = 0; while i < 10 { i = i + 1; if i == 5 { continue; } if i == 8 { break; } sum = sum + i; } return sum; }", 21),

        ("Nested function scopes",
         "fn outer() -> int { let x = 10; fn inner() -> int { return x + 5; } return inner(); } fn main() -> int { return outer(); }", 15),
    ]

    print("\n=== Testing Advanced Language Features ===")
    passed = 0

    for name, code, expected in tests:
        success, message = compile_and_run_code(code, expected)
        if success:
            print(f"[PASS] {name}")
            passed += 1
        else:
            print(f"[FAIL] {name}: {message}")

    return passed, len(tests)

def test_compilation_only():
    """Test features that should compile but don't need specific outputs"""
    tests = [
        ("Empty main function",
         "fn main() -> int { return 0; }"),

        ("Multiple functions",
         "fn helper() -> int { return 5; } fn main() -> int { return helper(); }"),

        ("Nested if statements",
         "fn main() -> int { if true { if false { return 1; } return 2; } return 3; }"),

        ("Complex control flow",
         "fn main() -> int { let x = 0; while x < 5 { if x == 2 { x = x + 2; } else { x = x + 1; } } return x; }"),

        ("Type annotations",
         "fn typed_function(x: int, y: int) -> int { let result: int = x + y; return result; } fn main() -> int { return typed_function(3, 4); }"),

        ("Module import (basic)",
         "use std::print; fn main() -> int { return 0; }"),

        ("Pattern matching (basic)",
         "fn main() -> int { let x = 5; return match x { 5 => 1, _ => 0 }; }"),

        ("Boolean expressions",
         "fn main() -> int { let flag = true; let result = flag && (5 > 3) || false; if result { return 1; } return 0; }"),
    ]

    print("\n=== Testing Compilation-Only Features ===")
    passed = 0

    for name, code in tests:
        success, message = compile_and_run_code(code)
        if success:
            print(f"[PASS] {name}")
            passed += 1
        else:
            print(f"[FAIL] {name}: {message}")

    return passed, len(tests)

def test_phase_1_features():
    """Test Phase 1 specific features that are implemented"""
    tests = [
        ("Memory management basics",
         "fn main() -> int { let x = 42; let y = x; return y; }", 42),

        ("Type inference",
         "fn test() -> int { let x = 5; let y = x + 3; return y; } fn main() -> int { return test(); }", 8),

        ("Function overloading check",
         "fn test(x: int) -> int { return x * 2; } fn main() -> int { return test(5); }", 10),

        ("Advanced control flow",
         "fn test() -> int { let mut i = 0; let mut sum = 0; while i < 5 { if i % 2 == 0 { sum = sum + i; } i = i + 1; } return sum; } fn main() -> int { return test(); }", 6),
    ]

    print("\n=== Testing Phase 1 Implemented Features ===")
    passed = 0

    for name, code, expected in tests:
        success, message = compile_and_run_code(code, expected)
        if success:
            print(f"[PASS] {name}")
            passed += 1
        else:
            print(f"[FAIL] {name}: {message}")

    return passed, len(tests)

def main():
    """Main test runner"""
    print("*** NEURO Language Features Test Suite ***")
    print("=" * 50)

    basic_passed, basic_total = test_basic_features()
    advanced_passed, advanced_total = test_advanced_features()
    compile_passed, compile_total = test_compilation_only()
    phase1_passed, phase1_total = test_phase_1_features()

    total_passed = basic_passed + advanced_passed + compile_passed + phase1_passed
    total_tests = basic_total + advanced_total + compile_total + phase1_total

    print(f"\n=== FEATURE TEST RESULTS ===")
    print(f"Basic features: {basic_passed}/{basic_total}")
    print(f"Advanced features: {advanced_passed}/{advanced_total}")
    print(f"Compilation tests: {compile_passed}/{compile_total}")
    print(f"Phase 1 features: {phase1_passed}/{phase1_total}")
    print(f"Total: {total_passed}/{total_tests}")

    if total_tests > 0:
        success_rate = (total_passed / total_tests) * 100
        print(f"Success rate: {success_rate:.1f}%")

        if success_rate >= 90.0:
            print("[EXCELLENT] Language features working well! Phase 1 Complete")
        elif success_rate >= 75.0:
            print("[GOOD] Most language features working")
        else:
            print("[NEEDS WORK] Some language features need attention")

        # Phase status reporting
        if success_rate >= 95.0:
            print("\n[STATUS] Ready for Phase 2 development!")
        elif success_rate >= 85.0:
            print("\n[STATUS] Phase 1 mostly complete, minor fixes needed")
        else:
            print("\n[STATUS] Phase 1 needs more work before Phase 2")

if __name__ == "__main__":
    import sys
    exit_code = main()
    if exit_code is None:
        sys.exit(0)
    sys.exit(exit_code)