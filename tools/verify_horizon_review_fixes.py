"""Static regression checks for Horizon review #1693; never launches Java or a game."""
from pathlib import Path
import hashlib
import json
import re
import struct
import tomllib

ROOT = Path(__file__).resolve().parents[1]
MOD = ROOT / 'horizon'
NS = 'useful_stuff'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify():
    manifest = tomllib.loads((MOD / 'horizon-mod.toml').read_text(encoding='utf-8'))
    assert manifest['id'] == NS and manifest['namespaces'] == [NS]
    assert manifest['version'] == '0.0.1'
    assert not (MOD / 'assets/things').exists()
    assert not (MOD / 'data/things').exists()
    parsed = {}
    for path in MOD.rglob('*'):
        if not path.is_file() or '.horizon' in path.parts or 'target' in path.parts:
            continue
        if path.suffix in {'.json', '.rs', '.toml'}:
            text = path.read_text(encoding='utf-8')
            assert not re.search(r'\bthings[:/]|\.things[."]', text), path
            if path.suffix == '.json':
                parsed[path] = json.loads(text)

    asset = MOD / 'assets' / NS
    for path, data in parsed.items():
        if path.is_relative_to(asset / 'models'):
            parent = data.get('parent', '')
            if parent.startswith(NS + ':'):
                assert (asset / 'models' / (parent.split(':', 1)[1] + '.json')).is_file()
            for texture in data.get('textures', {}).values():
                if texture.startswith(NS + ':'):
                    assert (asset / 'textures' / (texture.split(':', 1)[1] + '.png')).is_file()
        if path.is_relative_to(asset / 'items'):
            def models(value):
                if isinstance(value, dict):
                    model = value.get('model')
                    if isinstance(model, str) and model.startswith(NS + ':'):
                        assert (asset / 'models' / (model.split(':', 1)[1] + '.json')).is_file()
                    for child in value.values():
                        models(child)
                elif isinstance(value, list):
                    for child in value:
                        models(child)
            models(data)
    declared = MOD / 'data' / NS / 'horizon' / 'item'
    for path in declared.glob('*.json'):
        data = parsed[path]
        model = data['components']['minecraft:item_model']
        assert model.startswith(NS + ':')
        assert (asset / 'items' / (model.split(':', 1)[1] + '.json')).is_file()
    flint = json.loads((MOD / f'data/{NS}/horizon/entity_type/flint_shot.json').read_text())
    assert flint['item'] == NS + ':custom_shot'
    lang = parsed[asset / 'lang/en_us.json']
    assert 'enchantment.' + NS + '.grasping' in lang
    for path in declared.glob('*.json'):
        assert 'item.' + NS + '.' + path.stem in lang, path

    sprites = list((asset / 'textures/item').glob('*.png'))
    assert len(sprites) == 12
    for path in sprites:
        raw = path.read_bytes()
        assert raw[:8] == b'\x89PNG\r\n\x1a\n'
        assert struct.unpack('>II', raw[16:24]) == (16, 16), path
    art = ROOT / 'artwork/horizon-quiver'
    provenance = json.loads((art / 'provenance.json').read_text())
    quiver = asset / 'textures/item/quiver.png'
    assert provenance['referenceImages'] == []
    assert digest(art / 'source-generated.png') == provenance['sourceSha256']
    assert digest(quiver) == digest(art / 'quiver-16.png') == provenance['spriteSha256']
    assert digest(quiver) != provenance['rejectedSpriteSha256']
    print(f'Review checks passed: namespace {NS}, owned resource links, 12 sprites at 16x16, new original quiver.')


if __name__ == '__main__':
    verify()
