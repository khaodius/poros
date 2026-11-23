//! Reading and writing a vault's files as streams: stored bytes come in, are checked and
//! decrypted chunk by chunk, and leave as the file's contents, and the other way round.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::{Buf, Bytes, BytesMut};

use super::content::{FileHeader, CLEARTEXT_CHUNK};
use super::fs::{in_vault_terms, Keys};
use crate::error::{AppError, AppResult};
use crate::protocol::{ReadStream, WriteStream};

/// Decrypts the stored chunks a server stream delivers.
pub(super) struct VaultReader {
    source: Box<dyn ReadStream>,
    keys: Arc<Keys>,
    /// The file as the vault shows it, for errors.
    path: String,
    /// Absent until the stream, which then starts at the file's first byte, has delivered it.
    header: Option<FileHeader>,
    /// Stored bytes that do not yet make a whole chunk.
    stored: BytesMut,
    /// The number of the chunk `stored` starts with.
    chunk_number: u64,
    /// Bytes of the first chunk that come before the requested range.
    skip: usize,
    /// Bytes of the requested range not yet delivered.
    remaining: u64,
    source_ended: bool,
}

impl VaultReader {
    pub fn new(
        source: Box<dyn ReadStream>,
        keys: Arc<Keys>,
        path: String,
        header: Option<FileHeader>,
        first_chunk: u64,
        skip: usize,
        length: u64,
    ) -> Self {
        Self {
            source,
            keys,
            path,
            header,
            stored: BytesMut::new(),
            chunk_number: first_chunk,
            skip,
            remaining: length,
            source_ended: false,
        }
    }

    /// Decrypts every whole chunk in `stored`, and what is left once the source has ended,
    /// which is the file's last and shorter chunk.
    fn decrypt_available(&mut self, header: &FileHeader) -> AppResult<Vec<u8>> {
        let content = &self.keys.content;
        let encrypted_chunk = content.encrypted_chunk();
        let whole = self.stored.len() / encrypted_chunk;
        let mut cleartext = Vec::with_capacity((whole + 1) * CLEARTEXT_CHUNK);
        for _ in 0..whole {
            let chunk = self.stored.split_to(encrypted_chunk);
            content.decrypt_chunk(header, self.chunk_number, &chunk, &mut cleartext)?;
            self.chunk_number += 1;
        }
        if self.source_ended && !self.stored.is_empty() {
            let chunk = self.stored.split();
            content.decrypt_chunk(header, self.chunk_number, &chunk, &mut cleartext)?;
            self.chunk_number += 1;
        }
        Ok(cleartext)
    }

    async fn next_cleartext(&mut self) -> AppResult<Option<Bytes>> {
        loop {
            if self.remaining == 0 {
                return Ok(None);
            }
            let header_length = self.keys.content.header_length();
            if self.header.is_none() && self.stored.len() >= header_length {
                let stored_header = self.stored.split_to(header_length);
                self.header = Some(self.keys.content.decrypt_header(&stored_header)?);
            }
            let encrypted_chunk = self.keys.content.encrypted_chunk();
            let decryptable = self.stored.len() >= encrypted_chunk
                || (self.source_ended && !self.stored.is_empty());
            if let Some(header) = self.header.take_if(|_| decryptable) {
                let decrypted = self.decrypt_available(&header);
                self.header = Some(header);
                let mut cleartext = Bytes::from(decrypted?);
                let skipped = self.skip.min(cleartext.len());
                cleartext.advance(skipped);
                self.skip -= skipped;
                if cleartext.len() as u64 > self.remaining {
                    cleartext.truncate(self.remaining as usize);
                }
                self.remaining -= cleartext.len() as u64;
                if !cleartext.is_empty() {
                    return Ok(Some(cleartext));
                }
            }
            if self.source_ended {
                return match self.header {
                    Some(_) => Ok(None),
                    None => Err(AppError::integrity(
                        "The encrypted file is damaged: its header is cut short",
                    )),
                };
            }
            match self.source.next_chunk().await? {
                Some(bytes) => self.stored.extend_from_slice(&bytes),
                None => self.source_ended = true,
            }
        }
    }
}

#[async_trait]
impl ReadStream for VaultReader {
    async fn next_chunk(&mut self) -> AppResult<Option<Bytes>> {
        match self.next_cleartext().await {
            Ok(cleartext) => Ok(cleartext),
            Err(error) => Err(in_vault_terms(error, &self.path)),
        }
    }

    async fn finish(self: Box<Self>) -> AppResult<()> {
        let path = self.path;
        self.source
            .finish()
            .await
            .map_err(|error| in_vault_terms(error, &path))
    }
}

/// What a read of no bytes returns, once the file's header has been checked.
pub(super) struct NothingToRead;

#[async_trait]
impl ReadStream for NothingToRead {
    async fn next_chunk(&mut self) -> AppResult<Option<Bytes>> {
        Ok(None)
    }

    async fn finish(self: Box<Self>) -> AppResult<()> {
        Ok(())
    }
}

/// Encrypts a file's contents chunk by chunk into a server stream.
pub(super) struct VaultWriter {
    target: Box<dyn WriteStream>,
    keys: Arc<Keys>,
    /// The file as the vault shows it, for errors.
    path: String,
    header: FileHeader,
    /// The header's stored form, until the first write to the server carries it.
    stored_header: Option<Vec<u8>>,
    /// Contents that do not yet fill a chunk.
    pending: Vec<u8>,
    chunk_number: u64,
}

impl VaultWriter {
    pub fn new(target: Box<dyn WriteStream>, keys: Arc<Keys>, path: String) -> AppResult<Self> {
        let (header, stored_header) = keys.content.new_header()?;
        Ok(Self {
            target,
            keys,
            path,
            header,
            stored_header: Some(stored_header),
            pending: Vec::with_capacity(CLEARTEXT_CHUNK),
            chunk_number: 0,
        })
    }

    fn encrypt(&mut self, cleartext: &[u8], stored: &mut Vec<u8>) -> AppResult<()> {
        self.keys
            .content
            .encrypt_chunk(&self.header, self.chunk_number, cleartext, stored)?;
        self.chunk_number += 1;
        Ok(())
    }
}

#[async_trait]
impl WriteStream for VaultWriter {
    async fn write(&mut self, mut data: Bytes) -> AppResult<()> {
        let chunks = (self.pending.len() + data.len()) / CLEARTEXT_CHUNK;
        if chunks == 0 {
            self.pending.extend_from_slice(&data);
            return Ok(());
        }
        let mut stored = self.stored_header.take().unwrap_or_default();
        stored.reserve(chunks * self.keys.content.encrypted_chunk());
        if !self.pending.is_empty() {
            let filling = data.split_to(CLEARTEXT_CHUNK - self.pending.len());
            self.pending.extend_from_slice(&filling);
            let pending = std::mem::take(&mut self.pending);
            self.encrypt(&pending, &mut stored)?;
            self.pending = pending;
            self.pending.clear();
        }
        while data.len() >= CLEARTEXT_CHUNK {
            let chunk = data.split_to(CLEARTEXT_CHUNK);
            self.encrypt(&chunk, &mut stored)?;
        }
        self.pending.extend_from_slice(&data);
        self.target
            .write(Bytes::from(stored))
            .await
            .map_err(|error| in_vault_terms(error, &self.path))
    }

    async fn finish(mut self: Box<Self>) -> AppResult<()> {
        let mut stored = self.stored_header.take().unwrap_or_default();
        if !self.pending.is_empty() {
            let pending = std::mem::take(&mut self.pending);
            self.encrypt(&pending, &mut stored)?;
        }
        let Self {
            mut target, path, ..
        } = *self;
        let written = async {
            if !stored.is_empty() {
                target.write(Bytes::from(stored)).await?;
            }
            target.finish().await
        };
        written.await.map_err(|error| in_vault_terms(error, &path))
    }
}
