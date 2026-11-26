//! Resolves the direct hoster links a Hydra list can carry (PixelDrain, MediaFire,
//! MEGA) into a plain file URL the downloader understands. Plain HTTP(S) links and
//! torrents are handled elsewhere; GoFile is not supported yet because its API
//! needs a website token the Hydra obtains by running obfuscated JavaScript.

use cerberus_shared::tr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use aes::cipher::{BlockCipherDecrypt, BlockCipherEncrypt, KeyInit};
use aes::Aes128;
use base64::Engine;
use reqwest::Client;
use serde_json::{Value, json};

type Block = aes::Block;

use super::install;

/// Everything the downloader needs to fetch one file, whoever hosts it.
pub struct ResolvedDownload {
    /// The direct file URL (may expire; `identity` stays stable for resume).
    pub url: String,
    /// Stable key for the `.part` file, so a refreshed link still resumes.
    pub identity: String,
    /// The real file name, with its extension (drives how it is unpacked).
    pub name: String,
    /// Exact size in bytes, when the hoster says so; 0 otherwise.
    pub size: u64,
    pub headers: Vec<(String, String)>,
    /// Set for MEGA: the content is AES-CTR encrypted and must be decrypted while downloading.
    pub mega: Option<Mega>,
}

/// A MEGA file's key material and the expected plaintext size.
pub struct Mega {
    key: [u8; 16],
    nonce: [u8; 8],
    pub size: u64,
}

static SEQ: AtomicU64 = AtomicU64::new(0);

pub async fn resolve(client: &Client, hoster: &str, url: &str) -> Result<ResolvedDownload, String> {
    match hoster {
        "pixeldrain" => pixeldrain(client, url).await,
        "mediafire" => mediafire(client, url).await,
        "mega" => mega(client, url).await,
        _ => Err(tr!("Link não suportado.").into()),
    }
}

async fn pixeldrain(client: &Client, url: &str) -> Result<ResolvedDownload, String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| tr!("Link não suportado.").to_string())?;
    let segments: Vec<String> = parsed.path_segments().map(|segments| segments.map(str::to_owned).collect()).unwrap_or_default();
    let id = segments.get(1).filter(|_| segments.first().is_some_and(|s| s == "u")).cloned()
        .ok_or(tr!("Link não suportado.").to_string())?;
    let info_url = format!("https://pixeldrain.com/api/file/{id}/info");
    let info: Value = client.get(&info_url).send().await
        .map_err(|_| tr!("Não foi possível resolver o link do PixelDrain.").to_string())?
        .error_for_status()
        .map_err(|_| tr!("Não foi possível resolver o link do PixelDrain.").to_string())?
        .json().await
        .map_err(|_| tr!("Não foi possível resolver o link do PixelDrain.").to_string())?;
    if info.get("availability").is_some_and(|v| !v.is_null()) {
        return Err(tr!("Não foi possível resolver o link do PixelDrain.").into());
    }
    let name = info.get("name").and_then(Value::as_str).unwrap_or_default().to_owned();
    let size = info.get("size").and_then(Value::as_u64).unwrap_or(0);
    Ok(ResolvedDownload {
        url: format!("https://pixeldrain.com/api/file/{id}?download"),
        identity: url.to_owned(),
        name,
        size,
        headers: Vec::new(),
        mega: None,
    })
}

async fn mediafire(client: &Client, url: &str) -> Result<ResolvedDownload, String> {
    let page = client.get(url).send().await
        .map_err(|_| tr!("Não foi possível resolver o link do MediaFire.").to_string())?
        .error_for_status()
        .map_err(|_| tr!("Não foi possível resolver o link do MediaFire.").to_string())?
        .text().await
        .map_err(|_| tr!("Não foi possível resolver o link do MediaFire.").to_string())?;
    let found = mediafire_link(&page).ok_or_else(|| tr!("Não foi possível resolver o link do MediaFire."))?;
    let name = reqwest::Url::parse(&found).ok()
        .and_then(|u| u.path_segments().and_then(|mut p| p.next_back()).map(str::to_owned))
        .unwrap_or_default();
    Ok(ResolvedDownload {
        url: found,
        identity: url.to_owned(),
        name,
        size: 0,
        headers: Vec::new(),
        mega: None,
    })
}

/// Snips a `mediafire.com/…?dkey=…` or a `https://download…mediafire.com/…` link out of the page.
fn mediafire_link(html: &str) -> Option<String> {
    let mut fallback = None;
    for token in html.split(['\'', '"']) {
        let link = token.trim().trim_start_matches('\\');
        let candidate = if link.starts_with("//") { format!("https:{link}") } else { link.to_owned() };
        if !candidate.starts_with("http") || !candidate.contains("mediafire.com") {
            continue;
        }
        if candidate.contains("dkey=") {
            return Some(candidate);
        }
        let is_direct = candidate.split("://").nth(1).is_some_and(|rest| rest.starts_with("download"));
        if is_direct && fallback.is_none() {
            fallback = Some(candidate);
        }
    }
    fallback
}

async fn mega(client: &Client, url: &str) -> Result<ResolvedDownload, String> {
    let (id, key_b64) = mega_parts(url)?;
    let raw_key = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(key_b64.chars().filter(|c| *c != '=').collect::<String>())
        .map_err(|_| tr!("O link do MEGA não tem a chave de decriptação.").to_string())?;
    if raw_key.len() != 32 {
        return Err(tr!("O link do MEGA não tem a chave de decriptação.").into());
    }
    let key = fold_key(&raw_key);
    let nonce: [u8; 8] = raw_key[16..24].try_into().unwrap();
    let (download, size, attrs) = mega_file(client, &id).await?;
    let name = attrs_name(&key, &attrs)?;
    let mega = Mega { key, nonce, size };
    Ok(ResolvedDownload {
        url: download,
        identity: url.to_owned(),
        name,
        size,
        headers: Vec::new(),
        mega: Some(mega),
    })
}

/// The file id and its base64 key from a `/file/<id>#<key>` or `#!<id>!<key>` link.
fn mega_parts(url: &str) -> Result<(String, String), String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| tr!("Link do MEGA inválido.").to_string())?;
    let frag = parsed.fragment().unwrap_or_default();
    if let Some(rest) = frag.strip_prefix('!') {
        let mut it = rest.split('!');
        let id = it.next().unwrap_or_default().to_owned();
        let key = it.next().unwrap_or_default().to_owned();
        if id.is_empty() || key.is_empty() {
            return Err(tr!("Link do MEGA inválido.").into());
        }
        return Ok((id, key));
    }
    let id = parsed.path_segments().and_then(|mut p| p.next_back()).unwrap_or_default().to_owned();
    if id.is_empty() || frag.is_empty() {
        return Err(tr!("Link do MEGA inválido.").into());
    }
    Ok((id, frag.to_owned()))
}

/// The AES key MEGA derives by folding the 32-byte key: `key[i] = raw[i] ^ raw[i+16]`.
fn fold_key(raw: &[u8]) -> [u8; 16] {
    let mut key = [0u8; 16];
    for i in 0..16 {
        key[i] = raw[i] ^ raw[i + 16];
    }
    key
}

/// Asks MEGA for the file's transfer URL, size and encrypted attributes.
async fn mega_file(client: &Client, id: &str) -> Result<(String, u64, String), String> {
    let seq = SEQ.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
    let url = format!("https://g.api.mega.co.nz/cs?id={seq}");
    let body = json!([{ "a": "g", "g": 1, "ssl": 2, "p": id }]);
    let reply: Value = client.post(&url).json(&body).send().await
        .map_err(|_| tr!("Não foi possível resolver o link do MEGA.").to_string())?
        .json().await
        .map_err(|_| tr!("Não foi possível resolver o link do MEGA.").to_string())?;
    let first = reply.as_array().and_then(|a| a.first()).ok_or_else(|| tr!("Não foi possível resolver o link do MEGA."))?;
    let download = first.get("g").and_then(Value::as_str).ok_or_else(|| tr!("Não foi possível resolver o link do MEGA."))?.to_owned();
    let size = first.get("s").and_then(Value::as_u64).unwrap_or(0);
    let attrs = first.get("at").and_then(Value::as_str).unwrap_or_default().to_owned();
    Ok((download, size, attrs))
}

/// Decrypts the file's attribute block and reads its `n` (name).
fn attrs_name(key: &[u8; 16], attrs: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(attrs.chars().filter(|c| *c != '=').collect::<String>())
        .map_err(|_| tr!("Não foi possível resolver o link do MEGA.").to_string())?;
    if bytes.is_empty() || bytes.len() % 16 != 0 {
        return Err(tr!("Não foi possível resolver o link do MEGA.").into());
    }
    let mut plain = aes_cbc_decrypt(key, &bytes);
    if !plain.starts_with(b"MEGA") {
        return Err(tr!("Não foi possível resolver o link do MEGA.").into());
    }
    while plain.last() == Some(&0) {
        plain.pop();
    }
    let value: Value = serde_json::from_slice(&plain[4..]).map_err(|_| tr!("Não foi possível resolver o link do MEGA.").to_string())?;
    value.get("n").and_then(Value::as_str).map(str::to_owned).ok_or_else(|| tr!("Não foi possível resolver o link do MEGA.").to_string())
}

/// A single AES-128 block encrypt.
fn encrypt_block(cipher: &Aes128, block: [u8; 16]) -> [u8; 16] {
    let mut b = Block::from(block);
    cipher.encrypt_block(&mut b);
    b.into()
}

/// AES-128-CBC with a zero IV and no padding, for MEGA's short attribute blocks.
fn aes_cbc_decrypt(key: &[u8; 16], data: &[u8]) -> Vec<u8> {
    let cipher = Aes128::new_from_slice(key).expect("16-byte key");
    let mut prev = [0u8; 16];
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks(16) {
        let block: [u8; 16] = chunk.try_into().expect("16-byte blocks");
        let mut dec = Block::from(block);
        cipher.decrypt_block(&mut dec);
        let mut plain: [u8; 16] = dec.into();
        for i in 0..16 {
            plain[i] ^= prev[i];
        }
        prev = block;
        out.extend_from_slice(&plain);
    }
    out
}

/// AES-128-CTR over `data`, which starts at absolute plaintext `offset`, into `out`.
fn ctr_crypt(key: &[u8; 16], nonce: &[u8; 8], offset: u64, data: &[u8], out: &mut [u8]) {
    let cipher = Aes128::new_from_slice(key).expect("16-byte key");
    let mut block = offset / 16;
    let mut counter = [0u8; 16];
    counter[..8].copy_from_slice(nonce);
    counter[8..].copy_from_slice(&block.to_be_bytes());
    let mut ks = encrypt_block(&cipher, counter);
    let mut ki = (offset % 16) as usize;
    for (i, byte) in data.iter().enumerate() {
        out[i] = byte ^ ks[ki];
        ki += 1;
        if ki == 16 {
            block += 1;
            counter[8..].copy_from_slice(&block.to_be_bytes());
            ks = encrypt_block(&cipher, counter);
            ki = 0;
        }
    }
}

/// Downloads a MEGA file, decrypting the AES-CTR stream into a plaintext archive.
/// shortcut: no resume and no condensed-MAC check yet; TLS on the transfer URL
/// (forced via `ssl: 2`) is the only integrity guarantee for now.
pub async fn download_decrypted(
    client: &Client,
    mega: &Mega,
    url: &str,
    headers: &[(String, String)],
    dir: &Path,
    name: &str,
    cancel: &install::Halt,
    report: impl Fn(u64, u64) + Send,
) -> Result<PathBuf, String> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    tokio::fs::create_dir_all(dir).await.map_err(|e| e.to_string())?;
    let safe: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect();
    let target = dir.join(&safe);
    let part = dir.join(format!("{safe}.mega.part"));
    let mut request = client.get(url);
    for (key, value) in headers {
        request = request.header(key, value);
    }
    let reply = request.send().await.map_err(|e| e.to_string())?;
    if reply.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|v| v.to_ascii_lowercase().starts_with("text/html")) {
        return Err(tr!("O link abriu uma página em vez de um arquivo. Atualize a fonte ou tente outro link.").into());
    }
    let mut out = tokio::fs::File::create(&part).await.map_err(|e| e.to_string())?;
    let mut done = 0u64;
    let mut stream = reply.bytes_stream();
    let mut buf = vec![0u8; 1 << 20];
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        if cancel.stopped() {
            drop(out);
            let _ = std::fs::remove_file(&part);
            return Err(cerberus_shared::CANCELLED_INSTALL.into());
        }
        buf.resize(chunk.len(), 0);
        ctr_crypt(&mega.key, &mega.nonce, done, &chunk, &mut buf);
        out.write_all(&buf).await.map_err(|e| e.to_string())?;
        done += chunk.len() as u64;
        report(done, mega.size);
        if mega.size > 0 && done >= mega.size {
            break;
        }
    }
    out.flush().await.map_err(|e| e.to_string())?;
    if mega.size > 0 && done != mega.size {
        drop(out);
        let _ = std::fs::remove_file(&part);
        return Err(tr!("O arquivo do MEGA veio incompleto.").into());
    }
    super::throttle::rename_patiently(&part, &target, 20)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> [u8; 16] {
        [0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
    }

    #[test]
    fn the_mega_key_is_folded_xor_and_ctr_seeks_to_offsets() {
        let raw: Vec<u8> = (0..32).collect();
        let key = fold_key(&raw);
        for i in 0..16 {
            assert_eq!(key[i], i as u8 ^ (i as u8 + 16));
        }
        // CTR at offset 0 equals CTR at offset 16 shifted by one block's keystream.
        let nonce = [1u8; 8];
        let plain: Vec<u8> = (0..64).map(|i| i as u8).collect();
        let mut at0 = vec![0u8; 64];
        ctr_crypt(&key, &nonce, 0, &plain, &mut at0);
        let mut at16 = vec![0u8; 48];
        ctr_crypt(&key, &nonce, 16, &plain[16..], &mut at16);
        assert_eq!(&at0[16..], &at16[..]);
        // Encrypt+decrypt round-trips at a non-aligned offset.
        let mut back = vec![0u8; 48];
        ctr_crypt(&key, &nonce, 16, &at16, &mut back);
        assert_eq!(&back[..], &plain[16..]);
    }

    #[test]
    fn a_cbc_attribute_block_decrypts_and_keeps_the_name() {
        // "MEGA" + {"n":"game.zip"} zero-padded, encrypted here and decrypted back.
        let key = key();
        let cipher = Aes128::new_from_slice(&key).unwrap();
        let plain = b"MEGA{\"n\":\"game.zip\"}";
        let mut padded = [0u8; 32];
        padded[..plain.len()].copy_from_slice(plain);
        let mut prev = [0u8; 16];
        let mut ct = Vec::new();
        for chunk in padded.chunks(16) {
            let mut block = [0u8; 16];
            block.copy_from_slice(chunk);
            for i in 0..16 {
                block[i] ^= prev[i];
            }
            let enc = encrypt_block(&cipher, block);
            ct.extend_from_slice(&enc);
            prev = enc;
        }
        let attrs = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&ct);
        assert_eq!(attrs_name(&key, &attrs).unwrap(), "game.zip");
    }

    #[test]
    fn mega_links_give_their_id_and_key_in_both_formats() {
        let (id, key) = mega_parts("https://mega.nz/file/aBcD#xyz_-0123456789").unwrap();
        assert_eq!(id, "aBcD");
        assert_eq!(key, "xyz_-0123456789");
        let (id, key) = mega_parts("https://mega.nz/#!aBcD!xyz_-0123456789").unwrap();
        assert_eq!(id, "aBcD");
        assert_eq!(key, "xyz_-0123456789");
        assert!(mega_parts("https://mega.nz/file/aBcD").is_err());
    }
}
