extern crate yup_oauth2 as oauth;

use std::error;
use std::fmt;

use oauth::{AccessToken, InstalledFlowAuthenticator, InstalledFlowReturnMethod};
use reqwest::{header, Client, Method, Request, StatusCode, Url};
use serde::{Deserialize, Serialize};

/// Endpoint for the Google Sheets API.
const BASE_ENDPOINT: &str = "https://sheets.googleapis.com/v4/";

pub trait IntoSheetEntry {
    fn into_sheet_entry(&self) -> Vec<String>;
}

#[derive(Serialize)]
pub struct EmptyBody {}

pub struct Sheets {
    token: AccessToken,
    client: Client,
    sheet_id: String,
}

impl Sheets {
    pub fn get_link_to_sheet(sheet_id: &str) -> String {
        format!("https://docs.google.com/spreadsheets/d/{}/", sheet_id)
    }
    pub async fn initialize(sheet_id: &str) -> Result<Self, APIError> {
        let token = match Sheets::authenticate().await {
            Ok(t) => t,
            Err(e) => return Err(e),
        };

        Sheets::new(token, sheet_id)
    }

    pub async fn authenticate() -> Result<AccessToken, APIError> {
        // Read application secret from a file. Sometimes it's easier to compile it directly into
        // the binary. The `client_secret` file contains JSON like `{"installed":{"client_id": ... }}`
        let secret = oauth::read_application_secret("client_secret.json")
            .await
            .expect("client_secret.json");

        // All authentication tokens are persisted to a file named `tokencache.json`.
        // The authenticator takes care of caching tokens to disk and refreshing tokens once they've expired.
        let auth = match InstalledFlowAuthenticator::builder(
            secret,
            InstalledFlowReturnMethod::HTTPRedirect,
        )
        .persist_tokens_to_disk("tokencache.json")
        .build()
        .await
        {
            Ok(a) => a,
            Err(e) => return Err(APIError::from(e)),
        };

        let scope = &["https://www.googleapis.com/auth/spreadsheets"];
        match auth.token(scope).await {
            Ok(tok) => Ok(tok),
            Err(err) => panic!("Could not authenticate properly {:?}", err),
        }
    }

    pub fn new(token: AccessToken, sheet_id: &str) -> Result<Self, APIError> {
        match Client::builder().build() {
            Ok(client) => Ok(Self {
                token,
                client,
                sheet_id: String::from(sheet_id),
            }),
            Err(_) => Err(APIError {
                status_code: StatusCode::from_u16(500).unwrap(),
                body: "Could not instantiate client".to_string(),
            }),
        }
    }

    pub fn request<T>(
        &self,
        method: Method,
        path: &str,
        body: T,
        query_params: Option<Vec<(&str, &str)>>,
    ) -> Request
    where
        T: Serialize,
    {
        // confirm URL can parse before continuing
        let url = Url::parse(BASE_ENDPOINT).unwrap().join(&path).unwrap();

        // TODO: use `self.token = Sheets::authenticate().await;` to attempt to update token from cache
        // NOTE: this would make `request` async and I don't know if we want that or not
        if self.token.is_expired() {
            panic!("token is expired");
        }

        let bearer_token =
            header::HeaderValue::from_str(&format!("Bearer {}", &self.token.as_str())).unwrap();

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        headers.append(header::AUTHORIZATION, bearer_token);
        headers.append(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );

        let mut request_builder = self
            .client
            .request(Method::from(&method), url)
            .headers(headers);

        if let Some(val) = query_params {
            request_builder = request_builder.query(&val);
        }

        if method != Method::GET && method != Method::DELETE {
            request_builder = request_builder.json(&body);
        }

        request_builder.build().unwrap()
    }

    pub async fn clear_sheet(&self) -> Result<UpdateValuesResponse, APIError> {
        let request = self.request(
            Method::POST,
            &format!("spreadsheets/{}/values/Sheet1:clear", self.sheet_id),
            EmptyBody {},
            None,
        );

        let res = self.client.execute(request).await.unwrap();
        match res.status() {
            StatusCode::OK => Ok(res.json().await.unwrap()),
            s => Err(APIError {
                status_code: s,
                body: res.text().await.unwrap(),
            }),
        }
    }

    pub async fn refresh_entire_sheet(
        &self,
        value: Vec<Vec<String>>,
    ) -> Result<UpdateValuesResponse, APIError> {
        self.clear_sheet().await?;
        self.update_values("A1", value).await
    }

    pub async fn update_values(
        &self,
        range: &str,
        value: Vec<Vec<String>>,
    ) -> Result<UpdateValuesResponse, APIError> {
        let request = self.request(
            Method::PUT,
            &format!("spreadsheets/{}/values/{}", self.sheet_id, range),
            ValueRange {
                major_dimension: Some("ROWS".to_string()),
                range: Some(range.to_string()),
                values: Some(value),
            },
            Some(vec![
                ("valueInputOption", "USER_ENTERED"),
                ("responseValueRenderOption", "FORMATTED_VALUE"),
                ("responseDateTimeRenderOption", "FORMATTED_STRING"),
            ]),
        );

        let res = self.client.execute(request).await.unwrap();
        match res.status() {
            StatusCode::OK => Ok(res.json().await.unwrap()),
            status_code => Err(APIError {
                status_code,
                body: res.text().await.unwrap(),
            }),
        }
    }
}

#[derive(Debug)]
pub struct APIError {
    pub status_code: StatusCode,
    pub body: String,
}

impl fmt::Display for APIError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "APIError: '{}'\nbody: {}", self.status_code, self.body)
    }
}

impl error::Error for APIError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        // Generic error, underlying cause isn't tracked.
        None
    }
}

impl From<std::io::Error> for APIError {
    fn from(error: std::io::Error) -> Self {
        APIError {
            status_code: StatusCode::NOT_FOUND,
            body: format!("{}", error),
        }
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct ValueRange {
    pub range: Option<String>,
    pub values: Option<Vec<Vec<String>>>,
    #[serde(rename = "majorDimension")]
    pub major_dimension: Option<String>,
}

/// The response returned from updating values.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UpdateValuesResponse {
    #[serde(rename = "updatedColumns")]
    pub updated_columns: Option<i32>,
    #[serde(rename = "updatedRange")]
    pub updated_range: Option<String>,
    #[serde(rename = "updatedRows")]
    pub updated_rows: Option<i32>,
    #[serde(rename = "updatedData")]
    pub updated_data: Option<ValueRange>,
    #[serde(rename = "spreadsheetId")]
    pub spreadsheet_id: Option<String>,
    #[serde(rename = "updatedCells")]
    pub updated_cells: Option<i32>,
}

impl fmt::Display for UpdateValuesResponse {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} columns; {} rows; and {} total cells updated",
            self.updated_columns.unwrap_or(0),
            self.updated_rows.unwrap_or(0),
            self.updated_cells.unwrap_or(0)
        )
    }
}
