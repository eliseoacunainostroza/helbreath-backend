# Inventario de Funciones (`crates/net/src/lib.rs`)

Este documento lista todas las funciones/metodos definidos en `crates/net/src/lib.rs`,
incluyendo funciones de produccion, helpers internos y pruebas unitarias.

## 1) Metodos de `OpcodeMatrix`

| funcion | visibilidad | descripcion |
|---|---|---|
| `OpcodeMatrix::legacy_v382() -> Self` | `pub const fn` | Retorna la matriz de opcodes para protocolo legacy v3.82. |
| `OpcodeMatrix::modern_v400() -> Self` | `pub const fn` | Retorna la matriz de opcodes para protocolo modern v400 (hoy reutiliza legacy). |
| `OpcodeMatrix::for_version(version: ProtocolVersion) -> Self` | `pub const fn` | Selecciona la matriz segun la version de protocolo recibida. |

## 2) Metodos de `SessionPhase`

| funcion | visibilidad | descripcion |
|---|---|---|
| `SessionPhase::as_domain_state(self) -> SessionState` | `pub fn` | Convierte la fase de red a estado de sesion de dominio. |

## 3) Funciones publicas de red/traduccion

| funcion | visibilidad | descripcion |
|---|---|---|
| `decode_frame(frame: &[u8], max_payload: usize) -> Result<DecodedPacket, DecodeError>` | `pub fn` | Decodifica frame binario (`len + opcode + payload`) y valida tamano. |
| `encode_frame(opcode: u16, payload: &[u8]) -> Vec<u8>` | `pub fn` | Construye un frame binario para enviar al gateway. |
| `translate_packet(packet: &DecodedPacket, session_phase: SessionPhase) -> Result<ClientCommand, TranslateError>` | `pub fn` | Traduce paquete usando adaptador por defecto (`legacy_v382`). |
| `translate_packet_for_version(packet: &DecodedPacket, session_phase: SessionPhase, version: ProtocolVersion) -> Result<ClientCommand, TranslateError>` | `pub fn` | Traduce paquete a `ClientCommand` segun version de protocolo y valida fase de sesion. |
| `translate_server_packet(packet: &DecodedPacket) -> Result<ServerMessage, ServerTranslateError>` | `pub fn` | Traduce paquetes server->client usando la matriz legacy por defecto. |
| `translate_server_packet_for_version(packet: &DecodedPacket, version: ProtocolVersion) -> Result<ServerMessage, ServerTranslateError>` | `pub fn` | Traduce paquetes server->client con adaptador de version. |
| `split_frames(buffer: &mut BytesMut) -> Vec<Vec<u8>>` | `pub fn` | Extrae todos los frames completos disponibles desde un buffer incremental TCP. |

## 4) Funciones publicas de codec wire

| funcion | visibilidad | descripcion |
|---|---|---|
| `parse_wire_error_code(raw: u16) -> WireErrorCode` | `pub fn` | Mapea codigos wire-level a enum tipado de errores legacy. |
| `obfuscate_wire_payload(payload: &[u8], seed: u8) -> Vec<u8>` | `pub fn` | Aplica obfuscacion simetrica XOR por stream. |
| `deobfuscate_wire_payload(payload: &[u8], seed: u8) -> Vec<u8>` | `pub fn` | Revierte obfuscacion XOR por stream. |
| `compress_wire_payload(payload: &[u8]) -> Vec<u8>` | `pub fn` | Comprime payload con RLE wire-safe (escape `0xFF`). |
| `decompress_wire_payload(payload: &[u8], max_output: usize) -> Result<Vec<u8>, WireCodecError>` | `pub fn` | Descomprime payload RLE y valida limites. |
| `encode_wire_frame(opcode: u16, payload: &[u8], options: WireCodecOptions) -> Vec<u8>` | `pub fn` | Encapsula frame aplicando compresion/obfuscacion segun opciones. |
| `decode_wire_frame(frame: &[u8], max_payload: usize, options: WireCodecOptions) -> Result<DecodedPacket, WireDecodeError>` | `pub fn` | Decodifica frame y luego aplica deobfuscacion/descompresion wire. |

## 5) Metodos de `TokenBucketRateLimiter`

| funcion | visibilidad | descripcion |
|---|---|---|
| `TokenBucketRateLimiter::new(rate_per_sec: u32, burst: u32) -> Self` | `pub fn` | Inicializa rate limiter tipo token-bucket. |
| `TokenBucketRateLimiter::try_acquire(&mut self, cost: u32) -> bool` | `pub fn` | Intenta consumir tokens; retorna `true` si permite la operacion. |
| `TokenBucketRateLimiter::refill(&mut self)` | `fn` | Recalcula tokens disponibles segun tiempo transcurrido. |

## 6) Helpers internos de parseo

| funcion | visibilidad | descripcion |
|---|---|---|
| `parse_login_payload(payload: &[u8]) -> Result<LoginPayload, TranslateError>` | `fn` | Parsea `username/password/version` desde payload NUL-delimited. |
| `parse_character_create_payload(payload: &[u8]) -> Result<CharacterCreatePayload, TranslateError>` | `fn` | Parsea nombre/clase de creacion de personaje y aplica defaults de stats. |
| `parse_uuid_payload(payload: &[u8]) -> Result<Uuid, TranslateError>` | `fn` | Parsea UUID binario (16 bytes) o textual como fallback. |
| `parse_text_payload(payload: &[u8], max: usize) -> Result<String, TranslateError>` | `fn` | Parsea texto validado con limite maximo. |
| `parse_whisper_payload(payload: &[u8]) -> Result<(String, String), TranslateError>` | `fn` | Parsea whisper (`to_character`, `message`) separado por byte NUL. |
| `bytes_to_clean_string(input: &[u8], max: usize) -> Result<String, TranslateError>` | `fn` | Convierte bytes UTF-8 a string limpia, no vacia y con largo acotado. |

## 7) Funciones de prueba (`#[cfg(test)]`)

| funcion | descripcion |
|---|---|
| `decode_frame_ok()` | Verifica decodificacion valida de frame. |
| `decode_frame_invalid_len()` | Verifica error por largo declarado invalido. |
| `token_bucket_blocks_when_exhausted()` | Verifica bloqueo al agotar tokens del limiter. |
| `translate_character_delete_command()` | Verifica traduccion de opcode `character_delete` con UUID. |
| `translate_with_protocol_version_adapter()` | Verifica adaptador de version para `heartbeat` en `modern_v400`. |
| `wire_codec_roundtrip_with_compression_and_obfuscation()` | Verifica roundtrip wire con compresion + obfuscacion. |
| `translate_server_login_result()` | Verifica decoder tipado server->client para login result. |
| `parse_wire_error_code_unknown()` | Verifica fallback de codigos wire desconocidos. |
