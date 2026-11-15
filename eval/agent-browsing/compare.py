import argparse
import json
import math
import statistics
from collections import defaultdict
from pathlib import Path


def interval(passed, total, z=1.96):
    if not total:
        return "n/a"
    share = passed / total
    centre = (share + z * z / (2 * total)) / (1 + z * z / total)
    half = z * math.sqrt(share * (1 - share) / total + z * z / (4 * total * total)) / (1 + z * z / total)
    return f"{max(0, centre - half):.0%}–{min(1, centre + half):.0%}"


def median(values):
    values = [value for value in values if value is not None]
    return round(statistics.median(values), 1) if values else "n/a"


def main():
    parser = argparse.ArgumentParser(description="Summarise one or more results.json files from run.py")
    parser.add_argument("results", nargs="+", type=Path)
    args = parser.parse_args()
    runs = [run for path in args.results for run in json.loads(path.read_text())]
    scored = [run for run in runs if run.get("success") is not None]
    groups = defaultdict(list)
    for run in scored:
        groups[(run["version"], run["surface"])].append(run)
    print("| Version | Surface | Runs | Passed | 95% interval | Stuck | Recovered | False success | Median calls | Median s | Median tokens | Cost $ |")
    print("|---|---|---|---|---|---|---|---|---|---|---|---|")
    for (version, surface), group in sorted(groups.items()):
        passed = sum(bool(run["success"]) for run in group)
        print(f"| {version} | {surface} | {len(group)} | {passed} | {interval(passed, len(group))} | {sum(bool(run.get('stuck')) for run in group)} | {sum(bool(run.get('recovered')) for run in group)} | {sum(bool(run.get('false_success')) for run in group)} | {median([run.get('tool_calls') for run in group])} | {median([run.get('elapsed_s') for run in group])} | {median([run.get('usage', {}).get('totalTokens') for run in group])} | {sum(run.get('cost_usd', 0) for run in group):.2f} |")
    columns = sorted(groups)
    tasks = sorted({run["task"] for run in scored})
    print()
    print("| Task | " + " | ".join(f"{version} {surface}" for version, surface in columns) + " |")
    print("|---|" + "---|" * len(columns))
    for task in tasks:
        cells = []
        for column in columns:
            cell = [run for run in groups[column] if run["task"] == task]
            cells.append(f"{sum(bool(run['success']) for run in cell)}/{len(cell)}" if cell else "")
        print(f"| {task} | " + " | ".join(cells) + " |")
    failed = [run for run in runs if run.get("success") is None]
    if failed:
        print()
        print(f"Not scored (infrastructure errors): {len(failed)}")
        for run in failed:
            print(f"- {run['version']} {run['surface']} {run['task']}: {str(run.get('infrastructure_error'))[:160]}")


if __name__ == "__main__":
    main()
