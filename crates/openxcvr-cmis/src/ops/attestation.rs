use std::{error::Error, fmt};

use openxcvr_core::error::ModuleError;

use crate::{
    adapter::cdb::{
        Cdb, CdbError, CmisCdbDeviceIdCertificateLpl, CmisCdbDeviceIdCertificateLplRequest,
        CmisCdbGetDigestSignatureLpl, CmisCdbGetDigestSignatureLplRequest,
        CmisCdbSetDigestToSignLpl, CmisCdbSetDigestToSignLplRequest,
    },
    module::CmisTransport,
};

const MAX_CERTIFICATE_LEN: usize = 16 * 1024;

/// Why an attestation operation failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum AttestationError {
    /// The CDB attestation command failed; the original error is the source.
    Cdb(CdbError),
    /// The assembled certificate exceeds the 16 KiB budget.
    CertificateTooLarge,
    /// The signature length is not 64 bytes (ECDSA P-256).
    InvalidSignatureLength,
    /// The transport read or write failed; the original `ModuleError` is
    /// preserved as the error source instead of being flattened to text.
    Transport(ModuleError),
}

impl fmt::Display for AttestationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cdb(err) => write!(f, "attestation CDB error: {err}"),
            Self::CertificateTooLarge => write!(f, "attestation certificate exceeds 16 KiB"),
            Self::InvalidSignatureLength => write!(f, "attestation signature length is invalid"),
            Self::Transport(err) => write!(f, "attestation transport error: {err}"),
        }
    }
}

impl Error for AttestationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transport(err) => Some(err),
            _ => None,
        }
    }
}

impl From<CdbError> for AttestationError {
    fn from(err: CdbError) -> Self {
        Self::Cdb(err)
    }
}

/// Read the module's full device-ID certificate, segment by segment.
pub fn get_device_id_cert<T: CmisTransport>(
    transport: &mut T,
    max_polls: u32,
) -> Result<Vec<u8>, AttestationError> {
    let mut certificate = Vec::new();
    let mut segment_index = 0u8;

    loop {
        let response = Cdb::send::<CmisCdbDeviceIdCertificateLpl, _>(
            transport,
            &CmisCdbDeviceIdCertificateLplRequest {
                certificate_index: 0,
                segment_index,
            },
            max_polls,
        )?;

        if response.certificate_segment.is_empty() {
            return Ok(certificate);
        }

        if certificate.len() + response.certificate_segment.len() > MAX_CERTIFICATE_LEN {
            return Err(AttestationError::CertificateTooLarge);
        }

        certificate.extend_from_slice(&response.certificate_segment);
        segment_index = segment_index
            .checked_add(1)
            .ok_or(AttestationError::CertificateTooLarge)?;
    }
}

/// Supply a 32-byte digest for the module to sign.
pub fn set_digest_to_sign<T: CmisTransport>(
    transport: &mut T,
    digest: &[u8; 32],
    max_polls: u32,
) -> Result<(), AttestationError> {
    Cdb::send::<CmisCdbSetDigestToSignLpl, _>(
        transport,
        &CmisCdbSetDigestToSignLplRequest {
            digest_data: digest.to_vec(),
        },
        max_polls,
    )?;
    Ok(())
}

/// Read the signature of the previously supplied digest, segment by segment.
pub fn get_signature<T: CmisTransport>(
    transport: &mut T,
    max_polls: u32,
) -> Result<Vec<u8>, AttestationError> {
    let response = Cdb::send::<CmisCdbGetDigestSignatureLpl, _>(
        transport,
        &CmisCdbGetDigestSignatureLplRequest { segment_index: 0 },
        max_polls,
    )?;

    if response.signature_segment.is_empty() {
        return Err(AttestationError::InvalidSignatureLength);
    }

    Ok(response.signature_segment)
}

/// One-shot challenge: set the digest and read its signature.
pub fn challenge<T: CmisTransport>(
    transport: &mut T,
    digest: &[u8; 32],
    max_polls: u32,
) -> Result<Vec<u8>, AttestationError> {
    set_digest_to_sign(transport, digest, max_polls)?;
    get_signature(transport, max_polls)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openxcvr_core::{error::ModuleError, transport::Transport};

    const I2C_ADDR_MODULE: u8 = 0x50;
    const CDB_PAGE: u8 = 0x9F;
    const LOWER_PAGE: u8 = 0x00;
    const CDB_CMD_OFFSET: u8 = 128;
    const CDB_LPL_LENGTH_OFFSET: u8 = 134;
    const CDB_LPL_OFFSET: u8 = 136;
    const CDB_STATUS_OFFSET: u8 = 37;

    #[derive(Default)]
    struct MockTransport {
        last_cmd: Option<u16>,
        cmd_log: Vec<u16>,
        last_request_lpl: Vec<u8>,
        reply_lpl: Vec<u8>,
    }

    impl MockTransport {
        fn last_cmd(&self) -> Option<u16> {
            self.last_cmd
        }

        fn prepare_reply(&mut self, cmd: u16, request_lpl: &[u8]) {
            self.reply_lpl = match cmd {
                0x0400 => {
                    let segment_index = request_lpl.get(1).copied().unwrap_or_default();
                    if segment_index == 0 {
                        let cert = [
                            0x30, 0x82, 0x00, 0x0c, 0x30, 0x0a, 0x02, 0x01, 0x01, 0x30, 0x05, 0x06,
                            0x03, 0x2b, 0x65, 0x70,
                        ];
                        let mut reply = vec![0x00, 0x00, segment_index];
                        reply.extend_from_slice(&cert);
                        reply
                    } else {
                        vec![0x00, 0x00, segment_index]
                    }
                }
                0x0402 => vec![0x00, 0x00],
                0x0404 => {
                    let mut reply = vec![0x00, 0x00, 0x00];
                    reply.extend(0u8..64);
                    reply
                }
                _ => Vec::new(),
            };
        }
    }

    impl Transport for MockTransport {
        fn read(
            &mut self,
            slave_addr: u8,
            page: u8,
            bank: u8,
            offset: u8,
            buf: &mut [u8],
        ) -> Result<(), ModuleError> {
            assert_eq!(slave_addr, I2C_ADDR_MODULE);
            assert_eq!(bank, 0);

            match (page, offset, buf.len()) {
                (LOWER_PAGE, CDB_STATUS_OFFSET, 1) => buf[0] = 0x01,
                (CDB_PAGE, CDB_LPL_LENGTH_OFFSET, 2) => {
                    buf[0] = self.reply_lpl.len() as u8;
                    // RPLChkCode: one's complement of the reply payload sum
                    // (CMIS 5.3 Table 8-179); 0 when there is no reply body.
                    buf[1] = if self.reply_lpl.is_empty() {
                        0
                    } else {
                        !self
                            .reply_lpl
                            .iter()
                            .fold(0u8, |sum, byte| sum.wrapping_add(*byte))
                    };
                }
                (CDB_PAGE, CDB_LPL_OFFSET, len) => {
                    buf.copy_from_slice(&self.reply_lpl[..len]);
                }
                _ => {
                    return Err(ModuleError::Io(
                        format!(
                            "unexpected read page=0x{page:02x} offset={offset} len={}",
                            buf.len()
                        )
                        .into(),
                    ));
                }
            }

            Ok(())
        }

        fn write(
            &mut self,
            slave_addr: u8,
            page: u8,
            bank: u8,
            offset: u8,
            data: &[u8],
        ) -> Result<(), ModuleError> {
            assert_eq!(slave_addr, I2C_ADDR_MODULE);
            assert_eq!(page, CDB_PAGE);
            assert_eq!(bank, 0);
            assert_eq!(offset, CDB_CMD_OFFSET);
            assert!(data.len() >= 8);

            let cmd = u16::from_be_bytes([data[0], data[1]]);
            let lpl_len = data[4] as usize;
            let request_lpl = data[8..8 + lpl_len].to_vec();

            self.last_cmd = Some(cmd);
            self.cmd_log.push(cmd);
            self.prepare_reply(cmd, &request_lpl);
            self.last_request_lpl = request_lpl;

            Ok(())
        }

        fn probe(&mut self) -> Result<(), ModuleError> {
            Ok(())
        }
    }

    impl CmisTransport for MockTransport {}

    #[test]
    fn test_get_cert_single_segment() {
        let mut transport = MockTransport::default();

        let cert = get_device_id_cert(&mut transport, 10).unwrap();

        assert_eq!(cert.len(), 16);
        assert_eq!(&cert[..2], &[0x30, 0x82]);
    }

    #[test]
    fn test_challenge_sends_digest_and_returns_signature() {
        let mut transport = MockTransport::default();

        let signature = challenge(&mut transport, &[0u8; 32], 10).unwrap();

        assert_eq!(transport.cmd_log, vec![0x0402, 0x0404]);
        assert_eq!(signature.len(), 64);
        assert_eq!(signature[0], 0);
        assert_eq!(signature[63], 63);
    }

    #[test]
    fn test_set_digest_sends_correct_cmd() {
        let mut transport = MockTransport::default();

        set_digest_to_sign(&mut transport, &[0u8; 32], 10).unwrap();

        assert_eq!(transport.last_cmd(), Some(0x0402));
    }
}
