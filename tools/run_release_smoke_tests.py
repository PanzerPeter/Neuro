import argparse
import platform
import subprocess
import sys
import tempfile
from pathlib import Path


def run_command(command: list[str], cwd: Path | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=cwd,
        text=True,
        capture_output=True,
        check=False,
    )


def main() -> int:
    parser = argparse.ArgumentParser(description="Run release smoke tests for neurc.")
    parser.add_argument(
        "--neurc",
        type=Path,
        required=True,
        help="Path to neurc executable (typically target/release/neurc or neurc.exe).",
    )
    parser.add_argument(
        "--workspace",
        type=Path,
        default=Path.cwd(),
        help="Workspace root containing examples directory.",
    )
    args = parser.parse_args()

    neurc_path = args.neurc.resolve()
    workspace = args.workspace.resolve()

    if not neurc_path.exists():
        print(f"neurc executable not found: {neurc_path}")
        return 1

    examples = [
        ("milestone.nr", 8),
        ("factorial.nr", 120),
    ]

    is_windows = platform.system().lower().startswith("win")
    executable_suffix = ".exe" if is_windows else ""

    with tempfile.TemporaryDirectory() as temp_dir:
        temp_root = Path(temp_dir)

        for example_name, expected_exit in examples:
            source = workspace / "examples" / example_name
            output = temp_root / f"{source.stem}_smoke{executable_suffix}"

            compile_cmd = [
                str(neurc_path),
                "compile",
                str(source),
                "-O",
                "2",
                "-o",
                str(output),
            ]

            compile_result = run_command(compile_cmd, cwd=workspace)
            if compile_result.returncode != 0:
                print(f"Compile failed for {example_name} (exit={compile_result.returncode})")
                print(compile_result.stdout)
                print(compile_result.stderr)
                return 1

            run_result = run_command([str(output)], cwd=workspace)
            if run_result.returncode != expected_exit:
                print(
                    f"Smoke test failed for {example_name}: "
                    f"expected exit {expected_exit}, got {run_result.returncode}"
                )
                print(run_result.stdout)
                print(run_result.stderr)
                return 1

            print(f"Smoke test passed: {example_name} -> exit {run_result.returncode}")

    print("All release smoke tests passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())