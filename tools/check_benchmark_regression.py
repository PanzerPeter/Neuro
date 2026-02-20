import argparse
import json
import sys
from pathlib import Path


def load_json(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as file:
        return json.load(file)


def read_criterion_mean_ns(results_root: Path, benchmark_key: str) -> float:
    group, bench_name = benchmark_key.split("/", maxsplit=1)
    estimates_path = results_root / group / bench_name / "new" / "estimates.json"
    if not estimates_path.exists():
        raise FileNotFoundError(f"Missing benchmark output: {estimates_path}")

    estimates = load_json(estimates_path)
    mean = estimates.get("mean", {})
    point_estimate = mean.get("point_estimate")
    if point_estimate is None:
        raise ValueError(f"Invalid criterion estimates format in {estimates_path}")

    return float(point_estimate)


def main() -> int:
    parser = argparse.ArgumentParser(description="Fail CI on benchmark regressions.")
    parser.add_argument(
        "--baseline",
        type=Path,
        required=True,
        help="Path to baseline JSON mapping benchmark keys to nanosecond budgets.",
    )
    parser.add_argument(
        "--results-root",
        type=Path,
        default=Path("target/criterion"),
        help="Criterion output root directory (default: target/criterion).",
    )
    parser.add_argument(
        "--threshold-ratio",
        type=float,
        default=0.35,
        help="Allowed slowdown ratio over baseline (default: 0.35 = 35%%).",
    )
    args = parser.parse_args()

    baseline_data = load_json(args.baseline)
    benchmarks = baseline_data.get("benchmarks", {})
    if not benchmarks:
        print("No benchmark baseline entries found.")
        return 1

    failures: list[str] = []

    for benchmark_key, baseline_ns in benchmarks.items():
        measured_ns = read_criterion_mean_ns(args.results_root, benchmark_key)
        allowed_ns = float(baseline_ns) * (1.0 + args.threshold_ratio)
        slowdown = measured_ns / float(baseline_ns)

        print(
            f"{benchmark_key}: measured={measured_ns:.0f}ns "
            f"baseline={float(baseline_ns):.0f}ns allowed={allowed_ns:.0f}ns "
            f"ratio={slowdown:.2f}x"
        )

        if measured_ns > allowed_ns:
            failures.append(
                f"{benchmark_key} exceeded budget: measured {measured_ns:.0f}ns > allowed {allowed_ns:.0f}ns"
            )

    if failures:
        print("\nBenchmark regression check failed:")
        for failure in failures:
            print(f"  - {failure}")
        return 1

    print("\nBenchmark regression check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())