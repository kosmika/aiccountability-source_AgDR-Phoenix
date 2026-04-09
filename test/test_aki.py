import json
from agdr_aki import PhoenixKernel

def test_capture():
    engine = PhoenixKernel(":memory:", False)
    request = json.dumps({
        "ctx": {},
        "prompt": "test",
        "reasoning_trace": {},
        "output": "test",
        "ppp": {
            "provenance": "test",
            "place": "Toronto",
            "purpose": "test"
        },
        "human_delta_chain": None
    })
    result = json.loads(engine.capture(request, False))
    assert result["id"].startswith("aki_")
    assert 0.0 <= result["coherence_score"] <= 1.0
    print("✅ Basic test passed")
