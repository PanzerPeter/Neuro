#!/usr/bin/env python3
"""
Comprehensive test suite for NEURO compiler
Tests all .nr files in debug/ and examples/ directories
"""

import os
import subprocess
import sys
from pathlib import Path

def run_neurc_build(file_path):
    """Run neurc build on a file and return success status"""
    try:
        result = subprocess.run(
            ['./target/release/neurc', 'build', str(file_path)],
            cwd=Path(__file__).parent.parent,
            capture_output=True,
            text=True,
            timeout=30
        )
        return result.returncode == 0, result.stderr
    except subprocess.TimeoutExpired:
        return False, "Timeout"
    except Exception as e:
        return False, str(e)

def test_directory(directory):
    """Test all .nr files in a directory"""
    base_path = Path(__file__).parent.parent
    dir_path = base_path / directory

    if not dir_path.exists():
        print(f"Directory {directory} does not exist")
        return 0, 0, []

    nr_files = list(dir_path.glob("*.nr"))
    if not nr_files:
        print(f"No .nr files found in {directory}")
        return 0, 0, []

    success_count = 0
    failures = []

    print(f"\n=== Testing {directory}/ directory ===")

    for nr_file in sorted(nr_files):
        file_name = nr_file.name
        success, error = run_neurc_build(nr_file)

        if success:
            print(f"PASS {file_name}")
            success_count += 1
        else:
            print(f"FAIL {file_name}: {error}")
            failures.append((file_name, error))

    return success_count, len(nr_files), failures

def main():
    """Main test runner"""
    print("NEURO Compiler Test Suite")
    print("=" * 50)

    # Test debug directory
    debug_success, debug_total, debug_failures = test_directory("debug")

    # Test examples directory
    examples_success, examples_total, examples_failures = test_directory("examples")

    # Calculate totals
    total_success = debug_success + examples_success
    total_files = debug_total + examples_total

    print(f"\n=== FINAL RESULTS ===")
    print(f"Debug files: {debug_success}/{debug_total} successful")
    print(f"Examples files: {examples_success}/{examples_total} successful")
    print(f"Total: {total_success}/{total_files} successful")

    if total_files > 0:
        success_rate = (total_success / total_files) * 100
        print(f"Success rate: {success_rate:.1f}%")

        if success_rate >= 98.0:
            print("SUCCESS: Achieved 98%+ success rate!")
        else:
            print("FAILED: Did not achieve 98% success rate")

    # Report failures
    all_failures = debug_failures + examples_failures
    if all_failures:
        print(f"\n=== FAILURES ({len(all_failures)} files) ===")
        for file_name, error in all_failures:
            print(f"FAIL {file_name}: {error}")

    # Return appropriate exit code
    sys.exit(0 if success_rate >= 98.0 else 1)

if __name__ == "__main__":
    main()