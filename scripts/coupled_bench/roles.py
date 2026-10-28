"""Explicit role configuration for v2 matrices; v1 keeps its original defaults."""
import os
from urllib.parse import urlsplit

from .common import PROVIDERS, clean_env


def pair(profile):
    coder = {key: profile[key] for key in ("provider", "model")}
    return {"coder": coder, "observer": profile.get("observer", coder)}


def wire_limit(case):
    return case["max_requests"] + case.get("transport_repair_requests", 0)


def command_and_env(binary, folder, profile, case, matrix):
    roles = pair(profile)
    coder = roles["coder"]
    env = clean_env(binary)
    secrets = []
    for role in roles.values():
        key = PROVIDERS[role["provider"]][0]
        value = os.environ.get(key, "").strip()
        if not value:
            raise ValueError(f"Missing or blank {key}; credentials must come from the environment")
        env[key] = value
        secrets.append(value)
    command = [str(binary), "--provider", coder["provider"],
               "--base-url", PROVIDERS[coder["provider"]][1], "--model", coder["model"],
               "--temperature", str(matrix["temperature"]), "--max-tokens", str(matrix["max_tokens"]),
               "--timeout-seconds", str(matrix["timeout_seconds"]), "coupled-run",
               "--tool-root", str(folder / "workspace"), "--prompt-file", str(folder / "task.txt"),
               "--out-dir", str(folder / "run"), "--max-requests", str(case["max_requests"])]
    if matrix["version"] == 2:
        observer = roles["observer"]
        command += ["--observer-provider", observer["provider"], "--observer-model", observer["model"],
                    "--observer-base-url", PROVIDERS[observer["provider"]][1],
                    "--observer-api-key-env", PROVIDERS[observer["provider"]][0],
                    "--observer-max-tokens", str(matrix["observer_max_tokens"])]
        for field in ("coder_chunk_requests", "observer_reserved_requests", "transport_repair_requests"):
            command += ["--" + field.replace("_", "-"), str(case[field])]
    return command, env, secrets


def configuration_evidence(report, profile, case, matrix):
    """Qualify v2 only when the native receipts match the frozen roles/budgets."""
    work = report.get("work") or {}
    requests = work.get("requests")
    rows = requests if isinstance(requests, list) else []
    tasks = [r for r in rows if r.get("charge") == "task"]
    repairs = [r for r in rows if r.get("charge") == "transport"]
    roles = pair(profile)
    expected_roles = {}
    for name, role in roles.items():
        url = urlsplit(PROVIDERS[role["provider"]][1])
        expected_roles[name] = {
            **role, "transport": role["provider"] if role["provider"] in ("mistral", "anthropic") else "openai-compatible",
            "endpoint_origin": f"{url.scheme}://{url.netloc}",
            "temperature": matrix["temperature"] if name == "coder" else 0.2,
            "max_tokens": matrix["max_tokens"] if name == "coder" else matrix["observer_max_tokens"],
            "timeout_seconds": matrix["timeout_seconds"],
        }
    policy = {"transport_repair_limit": case["transport_repair_requests"],
              "observer_reserved_requests": case["observer_reserved_requests"]}
    counts = {name: sum(r.get("role") == name for r in rows) for name in roles}
    checks = {
        "roles_match": report.get("roles") == expected_roles,
        "same_model_flag": report.get("same_model_both_roles") == (roles["coder"] == roles["observer"]),
        "both_roles_called": all(counts.values()),
        "role_counts_match": report.get("role_requests") == counts,
        "known_roles_and_charges": len(tasks) + len(repairs) == len(rows)
            and all(r.get("role") in roles for r in rows),
        "native_policy_match": work.get("policy") == policy and work.get("request_limit") == case["max_requests"],
        "coder_chunk_match": report.get("coder_chunk_requests") == case["coder_chunk_requests"],
        "bounded_requests": bool(rows) and len(tasks) <= case["max_requests"]
            and len(repairs) <= case["transport_repair_requests"] and len(rows) <= wire_limit(case),
        "observer_reservation": sum(r.get("role") == "coder" for r in tasks)
            <= case["max_requests"] - case["observer_reserved_requests"],
        "transport_eligibility": all(r.get("status") == "completed" and
            r.get("transport_rejection") == ("native_arguments" if r.get("role") == "coder" else "observer_envelope")
            for r in repairs),
    }
    return {"valid": all(checks.values()), "checks": checks, "task_requests": len(tasks),
            "transport_requests": len(repairs), "wire_requests": len(rows) if isinstance(requests, list) else None,
            "expected_roles": expected_roles, "native_roles": report.get("roles")}
