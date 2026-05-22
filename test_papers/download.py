#!/usr/bin/env python3
"""Download test papers from their original sources."""

import os
import sys
import urllib.request
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed

PAPERS = {
    # Arxiv benchmark (20)
    "acl_2col_1.pdf": "https://aclanthology.org/2023.acl-long.352.pdf",
    "acm_llama2.pdf": "https://arxiv.org/pdf/2307.09288",
    "attention_is_all_you_need.pdf": "https://arxiv.org/pdf/1706.03762",
    "attention_mechanisms.pdf": "https://arxiv.org/pdf/2401.02954",
    "chain_of_thought.pdf": "https://arxiv.org/pdf/2201.11903",
    "cs_direct_preference.pdf": "https://arxiv.org/pdf/2306.05685",
    "cs_gpt4_techreport.pdf": "https://arxiv.org/pdf/2303.08774",
    "cs_mistral.pdf": "https://arxiv.org/pdf/2309.16609",
    "cs_rag_survey.pdf": "https://arxiv.org/pdf/2402.13228",
    "cs_tree_of_thoughts.pdf": "https://arxiv.org/pdf/2305.14314",
    "emnlp_2col_1.pdf": "https://aclanthology.org/2023.emnlp-main.148.pdf",
    "ieee_llm_agents.pdf": "https://arxiv.org/pdf/2401.14196",
    "llm_hallucination.pdf": "https://arxiv.org/pdf/2310.06825",
    "math_algebra.pdf": "https://arxiv.org/pdf/2401.06568",
    "math_optimization.pdf": "https://arxiv.org/pdf/2401.00891",
    "medical_imaging.pdf": "https://arxiv.org/pdf/2401.12114",
    "quantum_error.pdf": "https://arxiv.org/pdf/2312.17199",
    "short_phi2.pdf": "https://arxiv.org/pdf/2312.10997",
    "short_self_rag.pdf": "https://arxiv.org/pdf/2310.01798",
    "single_col_mamba.pdf": "https://arxiv.org/pdf/2312.00752",
    # ICSE 2026 (10)
    "icse_secure_reviewer.pdf": "https://arxiv.org/pdf/2510.26457",
    "icse_llm_diagnosis.pdf": "https://arxiv.org/pdf/2604.12108",
    "icse_flaky_tests.pdf": "https://arxiv.org/pdf/2601.08998",
    "icse_cps_fuzzing.pdf": "https://arxiv.org/pdf/2601.05449",
    "icse_misbehavior.pdf": "https://arxiv.org/pdf/2512.18823",
    "icse_code_review.pdf": "https://arxiv.org/pdf/2504.07459",
    "icse_perf_req.pdf": "https://arxiv.org/pdf/2511.03421",
    "icse_aibom.pdf": "https://arxiv.org/pdf/2510.07070",
    "icse_sat_linux.pdf": "https://raw.githubusercontent.com/SoftVarE-Group/Papers/main/2026/2026-ICSE-Kuiter.pdf",
    "icse_test_flakiness.pdf": "https://carolin-brandt.de/publications/vegelien-icseseip26.pdf",
    # CHI 2026 (10)
    "chi_privacy.pdf": "https://arxiv.org/pdf/2605.20206",
    "chi_point_grasp.pdf": "https://arxiv.org/pdf/2604.22491",
    "chi_gender.pdf": "https://arxiv.org/pdf/2604.15337",
    "chi_multi_ai.pdf": "https://arxiv.org/pdf/2603.26107",
    "chi_patient.pdf": "https://arxiv.org/pdf/2605.20205",
    "chi_twin_agents.pdf": "https://arxiv.org/pdf/2605.19838",
    "chi_xr_access.pdf": "https://arxiv.org/pdf/2602.17939",
    "chi_haptic.pdf": "https://arxiv.org/pdf/2503.08569",
    "chi_deception.pdf": "https://arxiv.org/pdf/2604.15338",
    "chi_viz.pdf": "https://arxiv.org/pdf/2601.19237",
}


def download(name, url, dest_dir):
    dest = dest_dir / name
    if dest.exists() and dest.stat().st_size > 10000:
        return name, "skipped (exists)"
    try:
        urllib.request.urlretrieve(url, dest)
        size = dest.stat().st_size
        if size < 10000:
            return name, f"WARNING: only {size} bytes"
        return name, "ok"
    except Exception as e:
        return name, f"FAILED: {e}"


def main():
    dest_dir = Path(__file__).parent
    print(f"Downloading {len(PAPERS)} test papers to {dest_dir}/")

    with ThreadPoolExecutor(max_workers=10) as pool:
        futures = {
            pool.submit(download, name, url, dest_dir): name
            for name, url in PAPERS.items()
        }
        ok = 0
        for future in as_completed(futures):
            name, status = future.result()
            if status == "ok":
                ok += 1
            elif "skipped" in status:
                ok += 1
            else:
                print(f"  {name}: {status}")

    total = len(list(dest_dir.glob("*.pdf")))
    print(f"Done: {total} papers in {dest_dir}/")


if __name__ == "__main__":
    main()
