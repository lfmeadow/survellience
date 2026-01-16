use crate::config::Config;
use anyhow::{Context, Result};
use polars::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct MmViabilityConfig {
    pub horizons_ms: Vec<i64>,
    pub fee_estimate: f64,
    pub eps: f64,
    pub min_rows: usize,
    pub max_mid_nan_frac: f64,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct GroupMetrics {
    venue: String,
    market_id: String,
    outcome_id: String,
    n_rows: usize,
    start_ts: i64,
    end_ts: i64,
    avg_spread: f64,
    p50_spread: f64,
    p90_spread: f64,
    avg_depth_top1: f64,
    avg_depth_top3: f64,
    quote_updates_per_minute: f64,
    mid_absdiff_per_sec: f64,
    mid_absdiff_per_min: f64,
    e_as_sell: HashMap<i64, f64>,
    e_as_buy: HashMap<i64, f64>,
    p90_as_sell: HashMap<i64, f64>,
    p90_as_buy: HashMap<i64, f64>,
    ev_avg: HashMap<i64, f64>,
    toxicity: HashMap<i64, f64>,
    trade_update_rate_per_minute: Option<f64>,
    trade_mid_deviation: Option<f64>,
    score: f64,
}

pub fn run_mm_viability(
    config: &Config,
    venue: &str,
    date: &str,
    hours: &str,
    fee_estimate: f64,
    min_rows: usize,
    top: usize,
    write_report: bool,
) -> Result<()> {
    let cfg = MmViabilityConfig {
        horizons_ms: vec![1000, 5000, 30000, 120000],
        fee_estimate,
        eps: 1e-9,
        min_rows,
        max_mid_nan_frac: 0.2,
    };

    let paths = collect_snapshot_paths(&config.data_dir, venue, date, hours)?;
    if paths.is_empty() {
        anyhow::bail!("No snapshot parquet files found for {} {}", venue, date);
    }

    let df = read_parquet_files(&paths)?;
    let report = compute_mm_viability(&df, &cfg)?;

    let report = rank_report(report)?;
    print_top(&report, top)?;

    if write_report {
        write_report_parquet(&config.data_dir, venue, date, &report)?;
    }

    Ok(())
}

fn collect_snapshot_paths(data_dir: &str, venue: &str, date: &str, hours: &str) -> Result<Vec<PathBuf>> {
    let base = Path::new(data_dir)
        .join("orderbook_snapshots")
        .join(format!("venue={}", venue))
        .join(format!("date={}", date));

    let hour_list = parse_hours(hours)?;
    let mut paths = Vec::new();
    for hour in hour_list {
        let hour_dir = base.join(format!("hour={:02}", hour));
        if !hour_dir.exists() {
            continue;
        }
        for entry in std::fs::read_dir(&hour_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("parquet") {
                paths.push(path);
            }
        }
    }
    Ok(paths)
}

fn parse_hours(hours: &str) -> Result<Vec<u32>> {
    if hours == "all" {
        return Ok((0..=23).collect());
    }
    if let Some((start, end)) = hours.split_once('-') {
        let start: u32 = start.parse().context("Invalid hours start")?;
        let end: u32 = end.parse().context("Invalid hours end")?;
        return Ok((start..=end).collect());
    }
    let hour: u32 = hours.parse().context("Invalid hours value")?;
    Ok(vec![hour])
}

fn read_parquet_files(paths: &[PathBuf]) -> Result<DataFrame> {
    let mut dfs = Vec::new();
    for path in paths {
        let file = std::fs::File::open(path)
            .with_context(|| format!("Failed to open {:?}", path))?;
        let df = ParquetReader::new(file)
            .finish()
            .with_context(|| format!("Failed to read {:?}", path))?;
        dfs.push(df);
    }
    let lfs: Vec<LazyFrame> = dfs.into_iter().map(|df| df.lazy()).collect();
    let df = concat(lfs, UnionArgs::default())
        .context("Failed to concat snapshot dataframes")?
        .collect()
        .context("Failed to collect snapshot dataframes")?;
    let df = df
        .sort(
            ["market_id", "outcome_id", "ts_recv"],
            SortMultipleOptions::new(),
        )
        .context("Failed to sort snapshots")?;
    Ok(df)
}

fn compute_mm_viability(df: &DataFrame, cfg: &MmViabilityConfig) -> Result<DataFrame> {
    let venue_value = df
        .column("venue")
        .ok()
        .and_then(|c| c.str().ok())
        .and_then(|c| c.get(0))
        .unwrap_or("unknown")
        .to_string();
    let market_id = df
        .column("market_id")?
        .str()
        .context("market_id not utf8")?;
    let outcome_id = df
        .column("outcome_id")?
        .str()
        .context("outcome_id not utf8")?;
    let ts_recv = df.column("ts_recv")?.i64().context("ts_recv not i64")?;
    let best_bid_px = df.column("best_bid_px")?.f64().context("best_bid_px not f64")?;
    let best_ask_px = df.column("best_ask_px")?.f64().context("best_ask_px not f64")?;
    let mid = df.column("mid")?.f64().context("mid not f64")?;
    let spread = df.column("spread")?.f64().context("spread not f64")?;
    let best_bid_sz = df.column("best_bid_sz")?.f64().context("best_bid_sz not f64")?;
    let best_ask_sz = df.column("best_ask_sz")?.f64().context("best_ask_sz not f64")?;

    let bid_sz_list = df.column("bid_sz")?.list().ok();
    let ask_sz_list = df.column("ask_sz")?.list().ok();

    let last_trade_px = df.column("last_trade_px").ok().and_then(|s| s.f64().ok());

    let mut rows = Vec::new();
    let mut i = 0usize;
    while i < df.height() {
        let m_id = market_id.get(i).unwrap_or("");
        let o_id = outcome_id.get(i).unwrap_or("");
        let mut j = i + 1;
        while j < df.height() {
            if market_id.get(j).unwrap_or("") != m_id || outcome_id.get(j).unwrap_or("") != o_id {
                break;
            }
            j += 1;
        }

        let group = compute_group_metrics(
            &venue_value,
            m_id,
            o_id,
            i,
            j,
            &ts_recv,
            &best_bid_px,
            &best_ask_px,
            &mid,
            &spread,
            &best_bid_sz,
            &best_ask_sz,
            bid_sz_list,
            ask_sz_list,
            last_trade_px,
            cfg,
        )?;
        if let Some(group) = group {
            rows.push(group);
        }
        i = j;
    }

    let mut market_ids = Vec::new();
    let mut outcome_ids = Vec::new();
    let mut venues = Vec::new();
    let mut n_rows = Vec::new();
    let mut start_ts = Vec::new();
    let mut end_ts = Vec::new();
    let mut avg_spread = Vec::new();
    let mut p50_spread = Vec::new();
    let mut p90_spread = Vec::new();
    let mut avg_depth_top1 = Vec::new();
    let mut avg_depth_top3 = Vec::new();
    let mut quote_updates_per_minute = Vec::new();
    let mut mid_absdiff_per_sec = Vec::new();
    let mut mid_absdiff_per_min = Vec::new();
    let mut trade_update_rate_per_minute = Vec::new();
    let mut trade_mid_deviation = Vec::new();
    let mut score = Vec::new();

    let mut e_as_sell_1s = Vec::new();
    let mut e_as_buy_1s = Vec::new();
    let mut e_as_sell_5s = Vec::new();
    let mut e_as_buy_5s = Vec::new();
    let mut e_as_sell_30s = Vec::new();
    let mut e_as_buy_30s = Vec::new();
    let mut e_as_sell_120s = Vec::new();
    let mut e_as_buy_120s = Vec::new();
    let mut ev_avg_5s = Vec::new();
    let mut ev_avg_30s = Vec::new();
    let mut ev_avg_120s = Vec::new();
    let mut toxicity_5s = Vec::new();
    let mut toxicity_30s = Vec::new();

    for row in rows {
        venues.push(row.venue);
        market_ids.push(row.market_id);
        outcome_ids.push(row.outcome_id);
        n_rows.push(row.n_rows as i64);
        start_ts.push(row.start_ts);
        end_ts.push(row.end_ts);
        avg_spread.push(row.avg_spread);
        p50_spread.push(row.p50_spread);
        p90_spread.push(row.p90_spread);
        avg_depth_top1.push(row.avg_depth_top1);
        avg_depth_top3.push(row.avg_depth_top3);
        quote_updates_per_minute.push(row.quote_updates_per_minute);
        mid_absdiff_per_sec.push(row.mid_absdiff_per_sec);
        mid_absdiff_per_min.push(row.mid_absdiff_per_min);
        trade_update_rate_per_minute.push(row.trade_update_rate_per_minute);
        trade_mid_deviation.push(row.trade_mid_deviation);
        score.push(row.score);

        e_as_sell_1s.push(*row.e_as_sell.get(&1000).unwrap_or(&f64::NAN));
        e_as_buy_1s.push(*row.e_as_buy.get(&1000).unwrap_or(&f64::NAN));
        e_as_sell_5s.push(*row.e_as_sell.get(&5000).unwrap_or(&f64::NAN));
        e_as_buy_5s.push(*row.e_as_buy.get(&5000).unwrap_or(&f64::NAN));
        e_as_sell_30s.push(*row.e_as_sell.get(&30000).unwrap_or(&f64::NAN));
        e_as_buy_30s.push(*row.e_as_buy.get(&30000).unwrap_or(&f64::NAN));
        e_as_sell_120s.push(*row.e_as_sell.get(&120000).unwrap_or(&f64::NAN));
        e_as_buy_120s.push(*row.e_as_buy.get(&120000).unwrap_or(&f64::NAN));
        ev_avg_5s.push(*row.ev_avg.get(&5000).unwrap_or(&f64::NAN));
        ev_avg_30s.push(*row.ev_avg.get(&30000).unwrap_or(&f64::NAN));
        ev_avg_120s.push(*row.ev_avg.get(&120000).unwrap_or(&f64::NAN));
        toxicity_5s.push(*row.toxicity.get(&5000).unwrap_or(&f64::NAN));
        toxicity_30s.push(*row.toxicity.get(&30000).unwrap_or(&f64::NAN));
    }

    let df = DataFrame::new(vec![
        Series::new("venue", venues),
        Series::new("market_id", market_ids),
        Series::new("outcome_id", outcome_ids),
        Series::new("n_rows", n_rows),
        Series::new("start_ts", start_ts),
        Series::new("end_ts", end_ts),
        Series::new("avg_spread", avg_spread),
        Series::new("p50_spread", p50_spread),
        Series::new("p90_spread", p90_spread),
        Series::new("avg_depth_top1", avg_depth_top1),
        Series::new("avg_depth_top3", avg_depth_top3),
        Series::new("quote_updates_per_minute", quote_updates_per_minute),
        Series::new("mid_absdiff_per_sec", mid_absdiff_per_sec),
        Series::new("mid_absdiff_per_min", mid_absdiff_per_min),
        Series::new("E_AS_sell_1s", e_as_sell_1s),
        Series::new("E_AS_buy_1s", e_as_buy_1s),
        Series::new("E_AS_sell_5s", e_as_sell_5s),
        Series::new("E_AS_buy_5s", e_as_buy_5s),
        Series::new("E_AS_sell_30s", e_as_sell_30s),
        Series::new("E_AS_buy_30s", e_as_buy_30s),
        Series::new("E_AS_sell_120s", e_as_sell_120s),
        Series::new("E_AS_buy_120s", e_as_buy_120s),
        Series::new("EV_avg_5s", ev_avg_5s),
        Series::new("EV_avg_30s", ev_avg_30s),
        Series::new("EV_avg_120s", ev_avg_120s),
        Series::new("toxicity_5s", toxicity_5s),
        Series::new("toxicity_30s", toxicity_30s),
        Series::new("trade_update_rate_per_minute", trade_update_rate_per_minute),
        Series::new("trade_mid_deviation", trade_mid_deviation),
        Series::new("score", score),
    ])
    .context("Failed to build report DataFrame")?;

    Ok(df)
}

fn compute_group_metrics(
    venue: &str,
    market_id: &str,
    outcome_id: &str,
    start: usize,
    end: usize,
    ts_recv: &Int64Chunked,
    best_bid_px: &Float64Chunked,
    best_ask_px: &Float64Chunked,
    mid: &Float64Chunked,
    spread: &Float64Chunked,
    best_bid_sz: &Float64Chunked,
    best_ask_sz: &Float64Chunked,
    bid_sz_list: Option<&ListChunked>,
    ask_sz_list: Option<&ListChunked>,
    last_trade_px: Option<&Float64Chunked>,
    cfg: &MmViabilityConfig,
) -> Result<Option<GroupMetrics>> {
    let n_rows = end - start;
    if n_rows == 0 {
        return Ok(None);
    }

    let mut ts = Vec::with_capacity(n_rows);
    let mut bid = Vec::with_capacity(n_rows);
    let mut ask = Vec::with_capacity(n_rows);
    let mut mid_vec = Vec::with_capacity(n_rows);
    let mut spread_vec = Vec::with_capacity(n_rows);
    let mut depth_top1 = Vec::with_capacity(n_rows);
    let mut depth_top3 = Vec::with_capacity(n_rows);
    let mut trade_px = Vec::with_capacity(n_rows);

    for idx in start..end {
        let t = ts_recv.get(idx).unwrap_or(0);
        let b = best_bid_px.get(idx).unwrap_or(f64::NAN);
        let a = best_ask_px.get(idx).unwrap_or(f64::NAN);
        let m = mid.get(idx).unwrap_or(f64::NAN);
        let s = spread.get(idx).unwrap_or(f64::NAN);
        let bsz = best_bid_sz.get(idx).unwrap_or(0.0);
        let asz = best_ask_sz.get(idx).unwrap_or(0.0);

        ts.push(t);
        bid.push(b);
        ask.push(a);
        mid_vec.push(m);
        spread_vec.push(s);
        depth_top1.push((bsz + asz) / 2.0);
        depth_top3.push(compute_depth_topk(bid_sz_list, ask_sz_list, idx, 3));

        if let Some(trade_col) = last_trade_px {
            trade_px.push(trade_col.get(idx));
        } else {
            trade_px.push(None);
        }
    }

    let start_ts = ts.first().cloned().unwrap_or(0);
    let end_ts = ts.last().cloned().unwrap_or(0);
    let time_range_ms = (end_ts - start_ts).max(1) as f64;
    let time_range_minutes = time_range_ms / 60000.0;
    let time_range_seconds = time_range_ms / 1000.0;

    let valid_spreads: Vec<f64> = spread_vec.iter().cloned().filter(|v| v.is_finite()).collect();
    let mid_valid_count = mid_vec.iter().filter(|v| v.is_finite()).count();
    let mid_nan_frac = 1.0 - (mid_valid_count as f64 / n_rows as f64);

    if n_rows < cfg.min_rows || mid_nan_frac > cfg.max_mid_nan_frac {
        return Ok(None);
    }

    let avg_spread = mean(&valid_spreads);
    if avg_spread <= 0.0 {
        return Ok(None);
    }

    let p50_spread = percentile(&valid_spreads, 0.5);
    let p90_spread = percentile(&valid_spreads, 0.9);

    let avg_depth_top1 = mean(&depth_top1);
    let avg_depth_top3 = mean(&depth_top3);

    let quote_updates_per_minute = n_rows as f64 / time_range_minutes;

    let mut total_abs_diff = 0.0;
    for idx in 1..mid_vec.len() {
        let prev = mid_vec[idx - 1];
        let cur = mid_vec[idx];
        if prev.is_finite() && cur.is_finite() {
            total_abs_diff += (cur - prev).abs();
        }
    }
    let mid_absdiff_per_sec = if time_range_seconds > 0.0 {
        total_abs_diff / time_range_seconds
    } else {
        0.0
    };
    let mid_absdiff_per_min = if time_range_minutes > 0.0 {
        total_abs_diff / time_range_minutes
    } else {
        0.0
    };

    let mut e_as_sell = HashMap::new();
    let mut e_as_buy = HashMap::new();
    let mut p90_as_sell = HashMap::new();
    let mut p90_as_buy = HashMap::new();
    let mut ev_avg = HashMap::new();
    let mut toxicity = HashMap::new();

    for &h in &cfg.horizons_ms {
        let mid_future = compute_forward_mid_for_horizon(&ts, &mid_vec, h);
        let mut as_sell = Vec::new();
        let mut as_buy = Vec::new();
        for i in 0..mid_vec.len() {
            let mf = mid_future[i];
            if !mf.is_finite() || !ask[i].is_finite() || !bid[i].is_finite() {
                continue;
            }
            as_sell.push(mf - ask[i]);
            as_buy.push(bid[i] - mf);
        }

        let e_sell = mean(&as_sell);
        let e_buy = mean(&as_buy);
        e_as_sell.insert(h, e_sell);
        e_as_buy.insert(h, e_buy);
        p90_as_sell.insert(h, percentile(&as_sell, 0.9));
        p90_as_buy.insert(h, percentile(&as_buy, 0.9));

        let gross_capture = avg_spread / 2.0;
        let ev_sell = gross_capture - e_sell - cfg.fee_estimate;
        let ev_buy = gross_capture - e_buy - cfg.fee_estimate;
        let ev = (ev_sell + ev_buy) / 2.0;
        ev_avg.insert(h, ev);

        let e_as_avg = (e_sell + e_buy) / 2.0;
        toxicity.insert(h, e_as_avg / (gross_capture + cfg.eps));
    }

    let (trade_update_rate_per_minute, trade_mid_deviation) =
        compute_trade_proxies(&trade_px, &mid_vec, time_range_minutes);

    let ev_30s = *ev_avg.get(&30000).unwrap_or(&0.0);
    let tox_30s = *toxicity.get(&30000).unwrap_or(&0.0);
    let score = 1000.0 * ev_30s
        + 0.05 * (avg_depth_top3 + 1.0).ln()
        + 0.01 * quote_updates_per_minute
        - 0.10 * tox_30s;

    Ok(Some(GroupMetrics {
        venue: venue.to_string(),
        market_id: market_id.to_string(),
        outcome_id: outcome_id.to_string(),
        n_rows,
        start_ts,
        end_ts,
        avg_spread,
        p50_spread,
        p90_spread,
        avg_depth_top1,
        avg_depth_top3,
        quote_updates_per_minute,
        mid_absdiff_per_sec,
        mid_absdiff_per_min,
        e_as_sell,
        e_as_buy,
        p90_as_sell,
        p90_as_buy,
        ev_avg,
        toxicity,
        trade_update_rate_per_minute,
        trade_mid_deviation,
        score,
    }))
}

fn compute_depth_topk(
    bid_sz_list: Option<&ListChunked>,
    ask_sz_list: Option<&ListChunked>,
    idx: usize,
    k: usize,
) -> f64 {
    let mut bid_sum = 0.0;
    let mut ask_sum = 0.0;
    if let Some(list) = bid_sz_list {
        if let Some(series) = list.get_as_series(idx) {
            if let Ok(vals) = series.f64() {
                bid_sum = vals.into_iter().take(k).flatten().sum();
            }
        }
    }
    if let Some(list) = ask_sz_list {
        if let Some(series) = list.get_as_series(idx) {
            if let Ok(vals) = series.f64() {
                ask_sum = vals.into_iter().take(k).flatten().sum();
            }
        }
    }
    (bid_sum + ask_sum) / 2.0
}

fn compute_trade_proxies(
    trade_px: &[Option<f64>],
    mid: &[f64],
    time_range_minutes: f64,
) -> (Option<f64>, Option<f64>) {
    let mut last_px: Option<f64> = None;
    let mut trade_updates = 0usize;
    let mut dev_sum = 0.0;
    let mut dev_count = 0usize;

    for i in 0..trade_px.len() {
        if let Some(px) = trade_px[i] {
            if last_px.map(|v| v != px).unwrap_or(true) {
                trade_updates += 1;
                last_px = Some(px);
            }
            if mid[i].is_finite() {
                dev_sum += (px - mid[i]).abs();
                dev_count += 1;
            }
        }
    }

    let trade_update_rate = if trade_updates > 0 && time_range_minutes > 0.0 {
        Some(trade_updates as f64 / time_range_minutes)
    } else {
        None
    };
    let trade_mid_deviation = if dev_count > 0 {
        Some(dev_sum / dev_count as f64)
    } else {
        None
    };

    (trade_update_rate, trade_mid_deviation)
}

fn compute_forward_mid_for_horizon(ts: &[i64], mid: &[f64], horizon_ms: i64) -> Vec<f64> {
    let mut result = vec![f64::NAN; ts.len()];
    let mut j = 0usize;
    for i in 0..ts.len() {
        let target = ts[i] + horizon_ms;
        while j < ts.len() && ts[j] < target {
            j += 1;
        }
        if j < ts.len() {
            result[i] = mid[j];
        }
    }
    result
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let sum: f64 = values.iter().sum();
    sum / values.len() as f64
}

fn percentile(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn rank_report(df: DataFrame) -> Result<DataFrame> {
    let mut df = df
        .sort(["score"], SortMultipleOptions::new())
        .context("Failed to sort by score")?;
    df = df.reverse();
    let rank: Vec<i64> = (1..=df.height() as i64).collect();
    df.with_column(Series::new("rank", rank))
        .context("Failed to add rank column")?;
    Ok(df)
}

fn print_top(df: &DataFrame, top: usize) -> Result<()> {
    let top_n = df.head(Some(top));
    let cols = [
        "rank",
        "market_id",
        "outcome_id",
        "avg_spread",
        "avg_depth_top3",
        "EV_avg_30s",
        "toxicity_30s",
        "quote_updates_per_minute",
        "trade_update_rate_per_minute",
    ];
    let selected = top_n.select(cols).context("Failed to select output columns")?;
    println!("{}", selected);
    Ok(())
}

fn write_report_parquet(data_dir: &str, venue: &str, date: &str, df: &DataFrame) -> Result<()> {
    let output_dir = Path::new(data_dir)
        .join("reports")
        .join("mm_viability")
        .join(format!("venue={}", venue))
        .join(format!("date={}", date));
    std::fs::create_dir_all(&output_dir)
        .with_context(|| format!("Failed to create report dir {:?}", output_dir))?;
    let file_path = output_dir.join("mm_viability.parquet");
    df.clone()
        .lazy()
        .sink_parquet(file_path.clone(), ParquetWriteOptions::default())
        .context("Failed to write report parquet")?;
    Ok(())
}
