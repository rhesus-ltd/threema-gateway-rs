use std::borrow::{Borrow, Cow};
use std::collections::HashMap;

use data_encoding::HEXLOWER_PERMISSIVE;

use crate::connection::{blob_upload, send_e2e, send_simple, Recipient};
use crate::crypto::{encrypt, encrypt_file_msg, encrypt_image_msg, encrypt_raw};
use crate::crypto::{EncryptedMessage, RecipientKey};
use crate::errors::{ApiBuilderError, ApiError};
use crate::lookup::{lookup_capabilities, lookup_credits, lookup_id, lookup_pubkey};
use crate::lookup::{Capabilities, LookupCriterion};
use crate::types::{BlobId, FileMessage, MessageType};
use crate::SecretKey;
use crate::MSGAPI_URL;

/// Implement methods available on both the simple and the e2e API objects.
macro_rules! impl_common_functionality {
    () => {
        /// Fetch the public key for the specified Threema ID.
        ///
        /// For the end-to-end encrypted mode, you need the public key of the recipient
        /// in order to encrypt a message. While it's best to obtain this directly from
        /// the recipient (extract it from the QR code), this may not be convenient,
        /// and therefore you can also look up the key associated with a given ID from
        /// the server.
        ///
        /// It is strongly recommended that you cache the public keys to avoid querying
        /// the API for each message.
        pub fn lookup_pubkey(&self, id: &str) -> Result<String, ApiError> {
            lookup_pubkey(self.endpoint.borrow(), &self.id, id, &self.secret)
        }

        /// Look up a Threema ID in the directory.
        ///
        /// An ID can be looked up either by a phone number or an e-mail
        /// address, in plaintext or hashed form. You can specify one of those
        /// criteria using the [`LookupCriterion`](enum.LookupCriterion.html)
        /// enum.
        pub fn lookup_id(&self, criterion: &LookupCriterion) -> Result<String, ApiError> {
            lookup_id(self.endpoint.borrow(), criterion, &self.id, &self.secret)
        }

        /// Look up the capabilities of a certain Threema ID.
        ///
        /// Before you send a file to a Threema ID using the blob upload (+file
        /// message), you may want to check whether the recipient uses a
        /// Threema version that supports receiving files. The receiver may be
        /// using an old version, or a platform where file reception is not
        /// supported.
        pub fn lookup_capabilities(&self, id: &str) -> Result<Capabilities, ApiError> {
            lookup_capabilities(self.endpoint.borrow(), &self.id, id, &self.secret)
        }

        /// Look up a remaining gateway credits.
        pub fn lookup_credits(&self) -> Result<i64, ApiError> {
            lookup_credits(self.endpoint.borrow(), &self.id, &self.secret)
        }
    };
}

/// Struct to talk to the simple API (without end-to-end encryption).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleApi {
    id: String,
    secret: String,
    endpoint: Cow<'static, str>,
}

impl SimpleApi {
    /// Initialize the simple API with the Gateway ID and the Gateway Secret.
    pub(crate) fn new<I: Into<String>, S: Into<String>>(
        endpoint: Cow<'static, str>,
        id: I,
        secret: S,
    ) -> Self {
        SimpleApi {
            id: id.into(),
            secret: secret.into(),
            endpoint,
        }
    }

    /// Send a message to the specified recipient in basic mode.
    ///
    /// Note that this mode of sending messages does not provide end-to-end
    /// encryption, only transport encryption between your host and the Threema
    /// Gateway server.
    ///
    /// Cost: 1 credit.
    pub fn send(&self, to: &Recipient, text: &str) -> Result<String, ApiError> {
        send_simple(self.endpoint.borrow(), &self.id, to, &self.secret, text)
    }

    impl_common_functionality!();
}

/// Struct to talk to the E2E API (with end-to-end encryption).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct E2eApi {
    id: String,
    secret: String,
    private_key: SecretKey,
    endpoint: Cow<'static, str>,
}

impl E2eApi {
    /// Initialize the simple API with the Gateway ID, the Gateway Secret and
    /// the Private Key.
    pub(crate) fn new<I: Into<String>, S: Into<String>>(
        endpoint: Cow<'static, str>,
        id: I,
        secret: S,
        private_key: SecretKey,
    ) -> Self {
        E2eApi {
            id: id.into(),
            secret: secret.into(),
            private_key,
            endpoint,
        }
    }

    /// Encrypt raw bytes for the specified recipient public key.
    pub fn encrypt_raw(&self, data: &[u8], recipient_key: &RecipientKey) -> EncryptedMessage {
        encrypt_raw(data, &recipient_key.0, &self.private_key)
    }

    /// Encrypt a text message for the specified recipient public key.
    pub fn encrypt_text_msg(&self, text: &str, recipient_key: &RecipientKey) -> EncryptedMessage {
        let data = text.as_bytes();
        let msgtype = MessageType::Text;
        encrypt(data, msgtype, &recipient_key.0, &self.private_key)
    }

    /// Encrypt an image message for the specified recipient public key.
    ///
    /// Before calling this function, you need to encrypt the image data (JPEG
    /// format) with [`encrypt_raw`](struct.E2eApi.html#method.encrypt_raw) and
    /// upload the ciphertext to the blob server.
    ///
    /// The image size needs to be specified in bytes. Note that the size is
    /// only used for download size displaying purposes and has no security
    /// implications.
    pub fn encrypt_image_msg(
        &self,
        blob_id: &BlobId,
        img_size_bytes: u32,
        image_data_nonce: &[u8; 24],
        recipient_key: &RecipientKey,
    ) -> EncryptedMessage {
        encrypt_image_msg(
            blob_id,
            img_size_bytes,
            image_data_nonce,
            &recipient_key.0,
            &self.private_key,
        )
    }

    /// Encrypt a file message for the specified recipient public key.
    ///
    /// To construct a [`FileMessage`], use [`FileMessageBuilder`].
    ///
    /// [`FileMessage`]: struct.FileMessage.html
    /// [`FileMessageBuilder`]: struct.FileMessageBuilder.html
    pub fn encrypt_file_msg(
        &self,
        msg: &FileMessage,
        recipient_key: &RecipientKey,
    ) -> EncryptedMessage {
        encrypt_file_msg(msg, &recipient_key.0, &self.private_key)
    }

    /// Send an encrypted E2E message to the specified Threema ID.
    ///
    /// If `delivery_receipts` is set to `false`, then the recipient's device will
    /// be instructed not to send any delivery receipts. This can be useful for
    /// one-way communication where the delivery receipt will be discarded. If
    /// you're unsure what value to use, set the flag to `false`.
    ///
    /// Cost: 1 credit.
    pub fn send(
        &self,
        to: &str,
        message: &EncryptedMessage,
        delivery_receipts: bool,
    ) -> Result<String, ApiError> {
        send_e2e(
            self.endpoint.borrow(),
            &self.id,
            to,
            &self.secret,
            &message.nonce,
            &message.ciphertext,
            delivery_receipts,
            None,
        )
    }

    /// Used for testing purposes. Not intended to be called by end users.
    #[doc(hidden)]
    pub fn send_with_params(
        &self,
        to: &str,
        message: &EncryptedMessage,
        delivery_receipts: bool,
        additional_params: HashMap<String, String>,
    ) -> Result<String, ApiError> {
        send_e2e(
            self.endpoint.borrow(),
            &self.id,
            to,
            &self.secret,
            &message.nonce,
            &message.ciphertext,
            delivery_receipts,
            Some(additional_params),
        )
    }

    impl_common_functionality!();

    /// Upload encrypted data to the blob server.
    ///
    /// If `persist` is set to `true`, then the blob will not be deleted
    /// after a client has downloaded it and marked it as done. Use when
    /// distributing the same blob to multiple clients.
    ///
    /// Cost: 1 credit.
    pub fn blob_upload(&self, data: &EncryptedMessage, persist: bool) -> Result<BlobId, ApiError> {
        blob_upload(
            self.endpoint.borrow(),
            &self.id,
            &self.secret,
            &data.ciphertext,
            persist,
            None,
        )
    }

    /// Used for testing purposes. Not intended to be called by end users.
    #[doc(hidden)]
    pub fn blob_upload_with_params(
        &self,
        data: &EncryptedMessage,
        persist: bool,
        additional_params: HashMap<String, String>,
    ) -> Result<BlobId, ApiError> {
        blob_upload(
            self.endpoint.borrow(),
            &self.id,
            &self.secret,
            &data.ciphertext,
            persist,
            Some(additional_params),
        )
    }

    /// Upload raw data to the blob server.
    ///
    /// If `persist` is set to `true`, then the blob will not be deleted
    /// after a client has downloaded it and marked it as done. Use when
    /// distributing the same blob to multiple clients.
    ///
    /// Cost: 1 credit.
    pub fn blob_upload_raw(&self, data: &[u8], persist: bool) -> Result<BlobId, ApiError> {
        blob_upload(
            self.endpoint.borrow(),
            &self.id,
            &self.secret,
            data,
            persist,
            None,
        )
    }

    /// Used for testing purposes. Not intended to be called by end users.
    #[doc(hidden)]
    pub fn blob_upload_raw_with_params(
        &self,
        data: &[u8],
        persist: bool,
        additional_params: HashMap<String, String>,
    ) -> Result<BlobId, ApiError> {
        blob_upload(
            self.endpoint.borrow(),
            &self.id,
            &self.secret,
            data,
            persist,
            Some(additional_params),
        )
    }
}

/// A convenient way to set up the API object.
///
/// # Examples
///
/// ## Simple API
///
/// ```
/// use threema_gateway::{ApiBuilder, SimpleApi};
///
/// let gateway_id = "*3MAGWID";
/// let gateway_secret = "hihghrg98h00ghrg";
///
/// let api: SimpleApi = ApiBuilder::new(gateway_id, gateway_secret).into_simple();
/// ```
///
/// ## E2E API
///
/// ```
/// use threema_gateway::{ApiBuilder, E2eApi};
///
/// let gateway_id = "*3MAGWID";
/// let gateway_secret = "hihghrg98h00ghrg";
/// let private_key = "998730fbcac1c57dbb181139de41d12835b3fae6af6acdf6ce91670262e88453";
///
/// let api: E2eApi = ApiBuilder::new(gateway_id, gateway_secret)
///                              .with_private_key_str(private_key)
///                              .and_then(|builder| builder.into_e2e())
///                              .unwrap();
/// ```
#[derive(Debug)]
pub struct ApiBuilder {
    pub id: String,
    pub secret: String,
    pub private_key: Option<SecretKey>,
    pub endpoint: Cow<'static, str>,
}

impl ApiBuilder {
    /// Initialize the ApiBuilder with the Gateway ID and the Gateway Secret.
    pub fn new<I: Into<String>, S: Into<String>>(id: I, secret: S) -> Self {
        ApiBuilder {
            id: id.into(),
            secret: secret.into(),
            private_key: None,
            endpoint: Cow::Borrowed(MSGAPI_URL),
        }
    }

    /// Set a custom API endpoint.
    ///
    /// The API endpoint should be a HTTPS URL without trailing slash.
    pub fn with_custom_endpoint<E: Into<Cow<'static, str>>>(mut self, endpoint: E) -> Self {
        let endpoint = endpoint.into();
        debug!("Using custom endpoint: {}", endpoint);
        if !(endpoint.starts_with("http:") || endpoint.starts_with("https:")) {
            warn!("Custom endpoint seems invalid!");
        }
        self.endpoint = endpoint;
        self
    }

    /// Return a [`SimpleAPI`](struct.SimpleApi.html) instance.
    pub fn into_simple(self) -> SimpleApi {
        SimpleApi::new(self.endpoint, self.id, self.secret)
    }

    /// Set the private key. Only needed for E2e mode.
    pub fn with_private_key(mut self, private_key: SecretKey) -> Self {
        self.private_key = Some(private_key);
        self
    }

    /// Set the private key from a byte slice. Only needed for E2e mode.
    pub fn with_private_key_bytes(mut self, private_key: &[u8]) -> Result<Self, ApiBuilderError> {
        let private_key = SecretKey::from_slice(private_key)
            .ok_or_else(|| ApiBuilderError::InvalidKey("Invalid libsodium private key".into()))?;
        self.private_key = Some(private_key);
        Ok(self)
    }

    /// Set the private key from a hex-encoded string reference. Only needed
    /// for E2e mode.
    pub fn with_private_key_str(self, private_key: &str) -> Result<Self, ApiBuilderError> {
        let private_key_bytes =
            HEXLOWER_PERMISSIVE
                .decode(private_key.as_bytes())
                .map_err(|e| {
                    let msg = format!("Could not decode private key hex string: {}", e);
                    ApiBuilderError::InvalidKey(msg)
                })?;
        self.with_private_key_bytes(&private_key_bytes)
    }

    /// Return a [`E2eAPI`](struct.SimpleApi.html) instance.
    pub fn into_e2e(self) -> Result<E2eApi, ApiBuilderError> {
        match self.private_key {
            Some(key) => Ok(E2eApi::new(self.endpoint, self.id, self.secret, key)),
            None => Err(ApiBuilderError::MissingKey),
        }
    }
}
