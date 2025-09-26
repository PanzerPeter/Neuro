#!/usr/bin/env python3
"""
Master test runner for NEURO compiler
Runs all test suites and provides comprehensive results
"""

import subprocess
import sys
from pathlib import Path

def run_test_script(script_name):
    """Run a test script and return its results"""
    script_path = Path(__file__).parent / script_name

    if not script_path.exists():
        return False, f"Test script {script_name} not found"

    try:
        result = subprocess.run(
            [sys.executable, str(script_path)],
            capture_output=True,
            text=True,
            timeout=300  # 5 minute timeout
        )

        return result.returncode == 0, result.stdout, result.stderr

    except subprocess.TimeoutExpired:
        return False, "", "Test timed out"
    except Exception as e:
        return False, "", str(e)

def main():
    """Main test runner"""
    print("*** NEURO Compiler - Master Test Suite ***")
    print("=" * 60)

    test_scripts = [
        ("test_all_nr_files.py", "File Compilation Tests"),
        ("test_compilation_features.py", "Language Feature Tests"),
    ]

    all_passed = True

    for script, description in test_scripts:
        print(f"\n[*] Running {description}")
        print("-" * 40)

        success, stdout, stderr = run_test_script(script)

        if success:
            print(f"[PASS] {description} - PASSED")
        else:
            print(f"[FAIL] {description} - FAILED")
            all_passed = False

        # Print the output from the test script
        if stdout:
            print(stdout)
        if stderr:
            print(f"Errors: {stderr}")

    print("\n" + "=" * 60)
    print("*** MASTER TEST SUITE RESULTS ***")
    print("=" * 60)

    if all_passed:
        print("[SUCCESS] ALL TESTS PASSED!")
        print("The NEURO compiler is working correctly.")
        print("[OK] Ready for production use")
    else:
        print("[ERROR] SOME TESTS FAILED")
        print("Review the test output above for details.")
        print("[WARNING] Not ready for production use")

    return 0 if all_passed else 1

if __name__ == "__main__":
    exit_code = main()
    sys.exit(exit_code)