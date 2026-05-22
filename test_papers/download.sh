#!/usr/bin/env bash
# Download test papers from their original sources (arxiv, author sites).
# PDFs are not included in this repo to avoid redistribution issues.

set -e
cd "$(dirname "$0")"

echo "Downloading test papers..."

# Arxiv benchmark papers (20)
curl -sL "https://aclanthology.org/2023.acl-long.352.pdf" -o acl_2col_1.pdf &
curl -sL "https://arxiv.org/pdf/2307.09288" -o acm_llama2.pdf &
curl -sL "https://arxiv.org/pdf/1706.03762" -o attention_is_all_you_need.pdf &
curl -sL "https://arxiv.org/pdf/2401.02954" -o attention_mechanisms.pdf &
curl -sL "https://arxiv.org/pdf/2201.11903" -o chain_of_thought.pdf &
curl -sL "https://arxiv.org/pdf/2306.05685" -o cs_direct_preference.pdf &
curl -sL "https://arxiv.org/pdf/2303.08774" -o cs_gpt4_techreport.pdf &
curl -sL "https://arxiv.org/pdf/2309.16609" -o cs_mistral.pdf &
curl -sL "https://arxiv.org/pdf/2402.13228" -o cs_rag_survey.pdf &
curl -sL "https://arxiv.org/pdf/2305.14314" -o cs_tree_of_thoughts.pdf &
curl -sL "https://aclanthology.org/2023.emnlp-main.148.pdf" -o emnlp_2col_1.pdf &
curl -sL "https://arxiv.org/pdf/2401.14196" -o ieee_llm_agents.pdf &
curl -sL "https://arxiv.org/pdf/2310.06825" -o llm_hallucination.pdf &
curl -sL "https://arxiv.org/pdf/2401.00891" -o math_algebra.pdf &
curl -sL "https://arxiv.org/pdf/2401.12114" -o math_optimization.pdf &
curl -sL "https://arxiv.org/pdf/2312.17199" -o medical_imaging.pdf &
curl -sL "https://arxiv.org/pdf/2312.17199" -o quantum_error.pdf &
curl -sL "https://arxiv.org/pdf/2312.10997" -o short_phi2.pdf &
curl -sL "https://arxiv.org/pdf/2310.01798" -o short_self_rag.pdf &
curl -sL "https://arxiv.org/pdf/2312.00752" -o single_col_mamba.pdf &

wait
echo "Arxiv papers: $(ls *.pdf 2>/dev/null | wc -l)"

# ICSE 2026 papers (10)
curl -sL "https://arxiv.org/pdf/2510.26457" -o icse_secure_reviewer.pdf &
curl -sL "https://arxiv.org/pdf/2604.12108" -o icse_llm_diagnosis.pdf &
curl -sL "https://arxiv.org/pdf/2601.08998" -o icse_flaky_tests.pdf &
curl -sL "https://arxiv.org/pdf/2601.05449" -o icse_cps_fuzzing.pdf &
curl -sL "https://arxiv.org/pdf/2512.18823" -o icse_misbehavior.pdf &
curl -sL "https://arxiv.org/pdf/2510.22530" -o icse_autocrashfl.pdf &
curl -sL "https://arxiv.org/pdf/2511.03421" -o icse_perf_req.pdf &
curl -sL "https://arxiv.org/pdf/2510.07070" -o icse_aibom.pdf &
curl -sL "https://raw.githubusercontent.com/SoftVarE-Group/Papers/main/2026/2026-ICSE-Kuiter.pdf" -o icse_sat_linux.pdf &
curl -sL "https://carolin-brandt.de/publications/vegelien-icseseip26.pdf" -o icse_test_flakiness.pdf &

wait
echo "ICSE papers: $(ls icse_*.pdf 2>/dev/null | wc -l)"

# CHI 2026 papers (10)
curl -sL "https://arxiv.org/pdf/2605.20206" -o chi_privacy.pdf &
curl -sL "https://arxiv.org/pdf/2604.22491" -o chi_point_grasp.pdf &
curl -sL "https://arxiv.org/pdf/2604.15337" -o chi_gender.pdf &
curl -sL "https://arxiv.org/pdf/2603.26107" -o chi_multi_ai.pdf &
curl -sL "https://arxiv.org/pdf/2605.20205" -o chi_patient.pdf &
curl -sL "https://arxiv.org/pdf/2605.19838" -o chi_twin_agents.pdf &
curl -sL "https://arxiv.org/pdf/2602.17939" -o chi_xr_access.pdf &
curl -sL "https://arxiv.org/pdf/2510.06573" -o chi_raven.pdf &
curl -sL "https://arxiv.org/pdf/2604.15338" -o chi_deception.pdf &
curl -sL "https://arxiv.org/pdf/2601.19237" -o chi_viz.pdf &

wait
echo "CHI papers: $(ls chi_*.pdf 2>/dev/null | wc -l)"

total=$(ls *.pdf 2>/dev/null | wc -l)
echo ""
echo "Done: $total papers downloaded to test_papers/"
