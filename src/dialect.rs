use rsomics_common::{Result, RsomicsError};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Dialect {
    pub(crate) delimiter: u8,
    pub(crate) comment: Option<u8>,
    pub(crate) header: bool,
}

impl Dialect {
    pub(crate) fn new(delimiter: u8, comment: Option<u8>, header: bool) -> Result<Self> {
        if !delimiter.is_ascii() || matches!(delimiter, b'\r' | b'\n' | b'"') {
            return Err(RsomicsError::ConfigError(
                "delimiter must be one ASCII byte other than CR, LF, or quote".to_owned(),
            ));
        }
        if let Some(comment) = comment
            && (!comment.is_ascii()
                || matches!(comment, b'\r' | b'\n' | b'"')
                || comment == delimiter)
        {
            return Err(RsomicsError::ConfigError(
                "comment must be a distinct ASCII byte other than CR, LF, or quote".to_owned(),
            ));
        }
        Ok(Self {
            delimiter,
            comment,
            header,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_ambiguous_bytes() {
        assert!(Dialect::new(b'\n', None, true).is_err());
        assert!(Dialect::new(b',', Some(b','), true).is_err());
        assert!(Dialect::new(b'"', None, true).is_err());
    }
}
