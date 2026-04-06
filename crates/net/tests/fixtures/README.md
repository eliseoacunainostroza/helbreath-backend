# Replay Fixtures

Place captured binary packet streams here (for example `replay_frames.bin`).
Each frame should follow the gateway framing convention:
- u16 length (little endian)
- u16 opcode (little endian)
- payload bytes

`replay_cases.json` is the always-on replay fixture used by CI/tests.
It contains:
- `phase`: session phase (`pre_auth`, `post_auth`, `in_character_list`, `in_world`, `closed`)
- `protocol_version` (opcional): `legacy_v382` (default) o `modern_v400`
- `origin` (opcional): `manual`, `capture`, `synthetic`, `seed`
- `frame_hex`: full frame bytes in hex
- `expect`: expected decode/translate result

Reglas de calidad aplicadas por test:
- nombres de casos unicos
- al menos un caso `decode_error`
- al menos un caso `translate_error`
- al menos un caso `modern_v400`
- cobertura minima de comandos base del protocolo
- al menos un comando `modern_v400` con origen `manual` o `capture`
- fixtures separados por version:
  - `replay_cases_legacy_v382.json`
  - `replay_cases_modern_v400.json`

Flujo recomendado desde captura real:

```bash
# opcional: capturar replay_frames.bin con proxy TCP local
CAPTURE_UPSTREAM=127.0.0.1:2848 make capture-replay

python3 deploy/scripts/replay_fixture_from_bin.py \
  --input crates/net/tests/fixtures/replay_frames.bin \
  --output crates/net/tests/fixtures/replay_cases.generated.json \
  --phase in_world \
  --protocol-version legacy_v382 \
  --expect-mode opcode_command \
  --auto-phase \
  --origin capture

python3 deploy/scripts/replay_merge_cases.py \
  --base crates/net/tests/fixtures/replay_cases.json \
  --incoming crates/net/tests/fixtures/replay_cases.generated.json \
  --output crates/net/tests/fixtures/replay_cases.json
```

Si aun no tienes cliente/captura real:

```bash
make replay-fixture-synth
make replay-fixture
make replay-fixture-merge
# opcional: sembrar cobertura modern provisional desde legacy
make replay-modern-seed
```
