import io
from pathlib import Path
from PIL import Image
import httpx
import pytest


@pytest.fixture
def png_bytes():
    buf = io.BytesIO()
    Image.new("RGB", (2000, 1000), (0, 128, 255)).save(buf, format="PNG")
    return buf.getvalue()


@pytest.fixture
def fake_remote_transport(png_bytes):
    return httpx.MockTransport(lambda req: httpx.Response(200, content=png_bytes))
