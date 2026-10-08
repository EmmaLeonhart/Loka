"""Tests for installer/gen_models.py (run: pytest installer/)."""

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import gen_models  # noqa: E402

TWO = """
[[model]]
id = "qwen-2.5-1.5b-instruct"
display_name = "Qwen 2.5 1.5B Instruct"
hf_repo = "Qwen/Qwen2.5-1.5B-Instruct"
approx_size = "3.1 GB"

[[model]]
id = "qwen-2.5-0.5b-instruct"
display_name = "Qwen 2.5 0.5B Instruct"
hf_repo = "Qwen/Qwen2.5-0.5B-Instruct"
approx_size = "1.0 GB"
"""


def test_one_exclusive_component_per_model_first_is_default():
    out = gen_models.render_components(gen_models.load_models(TWO))
    model_lines = [l for l in out.splitlines() if l.startswith('Name: "model\\')]
    assert len(model_lines) == 2
    assert all("Flags: exclusive" in l for l in model_lines)
    assert "Types: engine_model" in model_lines[0]
    assert "Types:" not in model_lines[1]  # only the first is preselected
    assert 'Name: "model\\qwen_2_5_1_5b_instruct"' in model_lines[0]
    assert "Qwen 2.5 0.5B Instruct (1.0 GB)" in model_lines[1]


def test_code_maps_each_component_to_its_id_and_repo():
    code = gen_models.render_code(gen_models.load_models(TWO))
    assert "IsComponentSelected('model\\qwen_2_5_0_5b_instruct')" in code
    assert "Id := 'qwen-2.5-0.5b-instruct';" in code
    assert "Repo := 'Qwen/Qwen2.5-0.5B-Instruct';" in code
    assert code.count("Result := True;") == 2


def test_the_real_models_toml_is_valid():
    real = (Path(gen_models.__file__).parent / "models.toml").read_text(encoding="utf-8")
    models = gen_models.load_models(real)
    assert len(models) >= 2
    assert len({m["id"] for m in models}) == len(models)


@pytest.mark.parametrize(
    "bad",
    [
        "",  # no models
        '[[model]]\nid = "a"\ndisplay_name = "A"\nhf_repo = "x/a"\n',  # missing approx_size
        '[[model]]\nid="a"\ndisplay_name="A"\nhf_repo="x/a"\napprox_size="1"\n'
        '[[model]]\nid="a"\ndisplay_name="B"\nhf_repo="x/b"\napprox_size="1"\n',  # duplicate id
        "[[model]]\nid='a'\ndisplay_name='A \"q\"'\nhf_repo='x/a'\napprox_size='1'\n",  # quote
    ],
)
def test_bad_models_toml_is_rejected(bad):
    with pytest.raises(ValueError):
        gen_models.load_models(bad)


def test_main_writes_both_includes(tmp_path):
    toml = tmp_path / "models.toml"
    toml.write_text(TWO, encoding="utf-8")
    assert gen_models.main(["gen", str(toml), str(tmp_path)]) == 0
    assert (tmp_path / "models.components.iss").read_text(encoding="utf-8").count("exclusive") == 2
    assert "function SelectedModel" in (tmp_path / "models.code.iss").read_text(encoding="utf-8")
