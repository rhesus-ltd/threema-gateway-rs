//! Error types used in this library.

use std::io::Error as IoError;
use reqwest::Error as ReqwestError;

quick_error! {
    /// Errors when interacting with the API.
    #[derive(Debug)]
    pub enum ApiError {
        /// The recipient identity is invalid or the account is not set up for basic mode
        BadSenderOrRecipient {}

        /// API identity or secret is incorrect
        BadCredentials {}

        /// No credits remain
        NoCredits {}

        /// Target ID not found
        IdNotFound {}

        /// Message is too long
        MessageTooLong {}

        /// Internal server error
        ServerError {}

        /// Wrong hash length
        BadHashLength {}

        /// Error when sending request (via reqwest)
        RequestError(err: ReqwestError) {
            from()
        }

        /// Error when reading response
        IoError(err: IoError) {
            from()
        }

        /// Other
        Other(msg: String) {
            from()
        }
    }
}

quick_error! {
    /// Crypto related errors.
    #[derive(Debug)]
    pub enum CryptoError {
        /// Bad key
        BadKey(msg: String) {
            from()
        }
    }
}
