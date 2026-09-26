"""Shared pytest configuration for the IntentForge API contract suites.

Two things live here, both about making CI honest rather than green:

1. `requires_upstream` marker registration.

   The suites in this directory assert TWO different things, and conflating them
   is what made the old `schema-regression` job vacuous:

     * GATEWAY CONTRACT  — the documented JSON shape of an endpoint. Assertable
       against a bare `gateway` container with no SearXNG / indexer / crawler.
       This is what CI can and must run on every push.
     * UPSTREAM-BEHAVIOUR — assertions that need real, non-empty upstream result
       rows (/images, /videos, /news, /shopping, recall-gap coverage). These
       need the full SearXNG stack, reach out to third-party engines, and are
       inherently non-deterministic on a shared runner, so they belong to the
       QA loop where the stack exists.

   Tests in the second class carry `@pytest.mark.requires_upstream`. CI selects
   `-m "not requires_upstream"`; the QA loop runs the whole directory with no
   marker filter and still exercises them.

   The marker must never be used to silence a failure in the first class. A
   gateway-contract test that starts skipping is a REGRESSION, and CI asserts a
   non-zero pass count so an all-skip run cannot report success.

2. `INTENTFORGE_REQUIRE_GATEWAY` is honoured uniformly.

   Each suite previously had its own private reachability fixture, and only one
   of them (the prefix-less goals file) honoured the env var. The other three
   skipped silently no matter what CI asked for. The `gateway_or_skip` fixture
   below is the single chokepoint: when the env var is "1" an unreachable
   gateway is a hard failure, never a skip.
"""

import os

import pytest

GATEWAY_ENV = "INTENTFORGE_REQUIRE_GATEWAY"


def pytest_configure(config):
    config.addinivalue_line(
        "markers",
        "requires_upstream: needs a live upstream result set (SearXNG/indexer); "
        "excluded from the CI gateway-contract run, exercised by the QA loop.",
    )


def _base_url() -> str:
    for var in ("INTENTFORGE_BASE_URL", "INTENTFORGE_BASE", "BASE_URL"):
        value = os.environ.get(var)
        if value:
            return value.rstrip("/")
    return "http://localhost:4000"


def gateway_required() -> bool:
    return os.environ.get(GATEWAY_ENV) == "1"


@pytest.fixture(scope="session")
def gateway_or_skip():
    """Fail (not skip) when CI declared the gateway mandatory but it is absent.

    This is the single guard every suite should use. A bare `if not reachable:
    skip` inside a suite is how a job ends up green while asserting nothing.
    """
    import urllib.error
    import urllib.request

    base = _base_url()
    required = gateway_required()
    body = "<no response>"
    alive = False
    try:
        with urllib.request.urlopen(f"{base}/health", timeout=10) as resp:
            body = resp.read().decode().strip()
            alive = resp.status == 200 and body == "OK"
    except Exception as exc:  # noqa: BLE001 - any transport error means "down"
        if required:
            pytest.fail(
                f"Gateway at {base} is REQUIRED ({GATEWAY_ENV}=1) but is not "
                f"reachable: {exc}. A required-gateway run must never skip."
            )
        pytest.skip(f"Gateway at {base} not reachable: {exc}")
    if not alive:
        if required:
            pytest.fail(f"Gateway at {base} /health did not return OK (got {body!r}).")
        pytest.skip(f"Gateway at {base} /health not OK ({body!r}).")
    return base
