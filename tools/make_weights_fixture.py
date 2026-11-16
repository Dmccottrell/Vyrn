"""Writes tests/fixtures/weights.safetensors with the reference library.

    pip install safetensors==0.8.0 numpy
    python3 tools/make_weights_fixture.py
"""

import os

import numpy as np
from safetensors.numpy import save_file

OUT = os.path.join(os.path.dirname(__file__), "..", "tests", "fixtures", "weights.safetensors")

save_file(
    {
        "layer.0.weight": np.arange(6, dtype=np.float32).reshape(2, 3) / 4,
        "layer.0.bias": np.array([0.5, -1.25, 3.0, 65504.0], dtype=np.float16),
        "positions": np.array([7, -9], dtype=np.int64),
        "scale": np.array(2.5, dtype=np.float64),
    },
    OUT,
    metadata={"format": "np"},
)
