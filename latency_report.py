#!/usr/bin/env python3
import argparse
from pathlib import Path

import polars as pl


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Compute latency stats from snapshot Parquet files."
    )
    parser.add_argument("--venue", default="polymarket", help="Venue name")
    parser.add_argument(
        "--date",
        default=None,
        help="UTC date (YYYY-MM-DD). Default: today in UTC.",
    )
    parser.add_argument(
        "--hour",
        default=None,
        help="Hour (HH, 00-23). If omitted, uses all hours.",
    )
    parser.add_argument(
        "--data-dir",
        default="data",
        help="Base data directory (default: data).",
    )
    return parser.parse_args()


def collect_files(base_dir: Path, hour: str | None) -> list[Path]:
    if hour is not None:
        hour_dir = base_dir / f"hour={hour.zfill(2)}"
        return sorted(hour_dir.glob("*.parquet")) if hour_dir.exists() else []
    files: list[Path] = []
    for hour_dir in sorted(base_dir.glob("hour=*")):
        if hour_dir.is_dir():
            files.extend(sorted(hour_dir.glob("*.parquet")))
    return files


def main() -> None:
    args = parse_args()
    date = args.date or pl.datetime.now(time_zone="UTC").strftime("%Y-%m-%d")
    base_dir = (
        Path(args.data_dir)
        / "orderbook_snapshots"
        / f"venue={args.venue}"
        / f"date={date}"
    )

    files = collect_files(base_dir, args.hour)
    if not files:
        print(f"No parquet files found under {base_dir}")
        return

    lf = pl.scan_parquet([str(p) for p in files]).select(
        [
            pl.col("ts_recv"),
            pl.col("source_ts"),
        ]
    )

    with_source = lf.filter(pl.col("source_ts").is_not_null()).with_columns(
        (pl.col("ts_recv") - pl.col("source_ts")).alias("latency_ms")
    )

    stats = (
        with_source.select(
            [
                pl.len().alias("rows_with_source_ts"),
                pl.col("latency_ms").min().alias("min_ms"),
                pl.col("latency_ms").mean().alias("mean_ms"),
                pl.col("latency_ms").median().alias("p50_ms"),
                pl.col("latency_ms").quantile(0.95, "nearest").alias("p95_ms"),
                pl.col("latency_ms").quantile(0.99, "nearest").alias("p99_ms"),
                pl.col("latency_ms").max().alias("max_ms"),
                (pl.col("latency_ms") < 0).sum().alias("negative_ms"),
            ]
        )
        .collect()
    )

    total_rows = lf.select(pl.len().alias("total_rows")).collect()
    total = total_rows.item(0, "total_rows")
    with_source_count = stats.item(0, "rows_with_source_ts")
    missing = total - with_source_count

    print(f"Venue: {args.venue}")
    print(f"Date: {date}")
    if args.hour is not None:
        print(f"Hour: {args.hour.zfill(2)}")
    print(f"Files: {len(files)}")
    print(f"Total rows: {total}")
    print(f"Rows with source_ts: {with_source_count}")
    print(f"Rows missing source_ts: {missing}")
    if with_source_count == 0:
        print("Latency (ms): no source_ts values available for latency calculation.")
        return
    print(
        "Latency (ms): "
        f"min={stats.item(0, 'min_ms'):.2f}, "
        f"mean={stats.item(0, 'mean_ms'):.2f}, "
        f"p50={stats.item(0, 'p50_ms'):.2f}, "
        f"p95={stats.item(0, 'p95_ms'):.2f}, "
        f"p99={stats.item(0, 'p99_ms'):.2f}, "
        f"max={stats.item(0, 'max_ms'):.2f}"
    )
    print(f"Negative latency rows: {stats.item(0, 'negative_ms')}")


if __name__ == "__main__":
    main()
