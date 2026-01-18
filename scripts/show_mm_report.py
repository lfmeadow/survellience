#!/usr/bin/env python3
"""Display MM viability report from parquet file"""
import sys
from pathlib import Path

try:
    import polars as pl
    df = pl.read_parquet(sys.argv[1])
    print(df)
except ImportError:
    try:
        import pandas as pd
        df = pd.read_parquet(sys.argv[1])
        pd.set_option('display.max_columns', None)
        pd.set_option('display.width', None)
        pd.set_option('display.max_colwidth', 80)
        print(df.to_string())
    except ImportError:
        print("Error: Need polars or pandas installed")
        print("Install with: pip install polars pyarrow")
        sys.exit(1)
