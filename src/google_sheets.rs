extern crate yup_oauth2 as oauth;

use std::fmt;

use oauth::{AccessToken, InstalledFlowAuthenticator, InstalledFlowReturnMethod};
use reqwest::{header, Client, Method, Request, StatusCode, Url};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};

/// Base endpoint for the Google Sheets API.
const BASE_ENDPOINT: &str = "https://sheets.googleapis.com/v4/";

/// used for convenient access the "A" side of "A1" notation
///
/// This is hard-coded because it'll compile to binary constant and be really nice and fast.
///
/// Take for example A1:C3, a range that spreads over 3 rows and (_1:_3) and 3 columns (A_:C_)
/// If we wanted to specify this range and we have a `Vec<Vec<u32>>` specifying the values
/// that should be placed in this range, we could make that range like so:
///
/// ```rust
/// data = vec![vec![1, 2, 3,], vec![4, 5, 6], vec![7, 8, 9]];
///
/// let start_column = ASCII_UPPER[0];
/// let end_column = ASCII_UPPER[data[0].len()];
/// let start_row = 0;
/// let end_row = data.len();
/// let range = format!("{}{}:{}{}", start_column, start_row, end_column, end_row);
/// println!("{}", range);  // -> A1:C3
/// ```
const ASCII_UPPER: [char; 26] = [
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S',
    'T', 'U', 'V', 'W', 'X', 'Y', 'Z',
];

/// helper function to get column notation ("A", "CF") from a zero-indexed number
///
/// For instance, the first column in a google sheets page is "A".
///
/// ```rust
/// let first_column_notation = get_column_notation(0);
/// println!("{}", &first_column_notation); // -> "A"
///
/// let further_column = get_column_notation(27);
/// println!("{}", &further_column);  // -> "AB";
///```
// TODO--
// write unit tests
fn get_column_notation(column: usize) -> String {
    if column < 26 {
        return format!("{}", ASCII_UPPER[column]);
    }

    // this will be rightmost value which will identify
    // which column this is (regardless of grouping)
    // i.e., column 28 would be associated "AB", so this would be the "B"
    let alpha_overflow = column % 26;
    // this will be column grouping or repetition
    //i.e., column 28 would be associated "AB", so this would be the "A"
    let column_rep = column / 26;

    let mut st = String::new();
    if column_rep > 26 {
        for _ in 0..((column_rep / 26) % 26) + 1 {
            st.push(ASCII_UPPER[(column_rep / 26) % 26 - 1]);
        }
    } else {
        st.push(ASCII_UPPER[column_rep - 1]);
    }

    format!("{}{}", st, ASCII_UPPER[alpha_overflow])
}

#[test]
fn test_column_notation() {
    assert_eq!(get_column_notation(0), "A");
    assert_eq!(get_column_notation(3), "D");
    assert_eq!(get_column_notation(25), "Z");
    assert_eq!(get_column_notation(26), "AA");
    assert_eq!(get_column_notation(52), "BA");
    assert_eq!(get_column_notation(702), "AAA");
    assert_eq!(get_column_notation(703), "AAB");
}

/// This is a helper function to retrieve valid A1 notation given the starting and ending index for
/// columns and rows in a zero-index fashion. This is used in zero-index fashion to make it easier to work
/// with arrays `Vec` of data!
///
/// Please refer to [Google Sheets Docs: A1 Notation] for more information on A1 Notation
///
/// # Examples
///
/// ```rust
/// const top_left_nine_cells = get_a1_notation(Some(0), Some(0), Some(2), Some(2));
/// println!("{}", top_left_nine_cells);  // -> "A1:C3"
///
/// const rows_five_through_nine = get_a1_notation(None, Some(4), None, Some(8));
/// println!("{}", rows_five_through_nine); // -> "5:9"
///
/// const rows_five_through_nine_third_column_on = get_a1_notation(None, Some(4), Some(2), Some(8));
/// println!("{}", rows_five_through_nine_third_column_on); // -> "5:C9"
/// ```
///
/// [Google Sheets Docs: A1 Notation]: https://developers.google.com/sheets/api/guides/concepts#expandable-1
// TODO--
// write unit tests
pub fn get_a1_notation(
    start_column: Option<usize>,
    start_row: Option<usize>,
    end_column: Option<usize>,
    end_row: Option<usize>,
) -> String {
    match (start_column, start_row, end_column, end_row) {
        // "A5:A" refers to all the cells in the first column, from row 5 onward
        (Some(sc), Some(r), Some(ec), None) |
        // "A:A5" is not technically valid, but defaults to "A5:A"
        (Some(sc), None, Some(ec), Some(r)) => {
            format!("{}{}:{}", sc, r+1, get_column_notation(ec))
        },
        // "A1:B2" refers to the first two cells in the top two rows
        (Some(sc), Some(sr), Some(ec), Some(er)) => {
            format!("{}{}:{}{}", get_column_notation(sc), sr+1, get_column_notation(ec), er+1)
        },
        // "A:B" refers to all the cells in the first two columns
        (Some(sc), _, Some(ec), _) => {
            format!("{}:{}", get_column_notation(sc), get_column_notation(ec))
        },
        // "10:18" refers to all cells in rows 10 through 18
        // "10:B18" refers to all cells in rows 10 through 18, from column B onward
        (None, Some(sr), possible_column, Some(er)) => {
            if let Some(column) = possible_column {
                // refers to all cells in given rows
                format!("{}{}:{}", get_column_notation(column), sr+1, er+1)
            } else {
                format!("{}:{}", sr+1, er+1)
            }
        },
        _ => {
            panic!("The specified range is not valid")
        }
    }
}

pub struct Sheets {
    token: AccessToken,
    client: Client,
    sheet_id: String,
}

type Result<T, E = ApiError> = std::result::Result<T, E>;

impl Sheets {
    pub fn new(token: AccessToken, sheet_id: &str) -> Result<Self> {
        let client = Client::builder().build().context(ClientBuildFail {})?;

        Ok(Self {
            token,
            client,
            sheet_id: String::from(sheet_id),
        })
    }

    pub async fn initialize(sheet_id: &str) -> Result<Self> {
        let token = Sheets::authenticate().await?;
        Sheets::new(token, sheet_id)
    }

    pub fn get_link_to_sheet(&self) -> String {
        format!("https://docs.google.com/spreadsheets/d/{}/", self.sheet_id)
    }

    pub async fn authenticate() -> Result<AccessToken> {
        // Read application secret from a file. Sometimes it's easier to compile it directly into the binary.
        let secret = oauth::read_application_secret("client_secret.json")
            .await
            .context(AuthenticateError {
                meta: "Failed to configure secret from 'client_secret.json'",
            })?;

        // All authentication tokens are persisted to a file named `tokencache.json`.
        // The authenticator takes care of caching tokens to disk and refreshing tokens once they've expired.
        let auth =
            InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::HTTPRedirect)
                .persist_tokens_to_disk("tokencache.json")
                .build()
                .await
                .context(AuthenticateError { meta: "Failed to build auth from secret. Try deleting 'tokencache.json' and running again."})?;

        let scope = &["https://www.googleapis.com/auth/spreadsheets"];

        let token = auth.token(scope).await.context(TokenError {
            scope: String::from(scope[0]),
        })?;

        Ok(token)
    }

    /// Makes a request to the Google Sheets API
    ///
    /// # Arguments
    ///
    /// - `method`: The type of request to make (GET, POST, etc.)
    /// - `path`: The path to the endpoint (for example: `spreadsheets/{spreadsheetId}/values/{range}:append`)
    /// - `body`: The body of the request
    /// - `query_params`: The query parameters to add on to the request, in a list of tuples with `Vec<(parameter_name, parameter_value)>`
    async fn request<T: Serialize>(
        &self,
        method: Method,
        path: &str,
        body: T,
        query_params: Option<Vec<(&str, &str)>>,
    ) -> Request {
        // confirm URL can parse before continuing
        let url = Url::parse(BASE_ENDPOINT).unwrap().join(&path).unwrap();

        // TODO-- use `self.token = Sheets::authenticate().await.unwrap()` to attempt to read token from cache
        // Note: this would require a mutable reference to `&mut self` in practically every method for `google_sheets::Sheets`
        if self.token.is_expired() {
            panic!("Token is expired");
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

    /// Appends values within new row under existing data.
    ///
    /// See [Google Sheets Docs: `spreadsheets.values.append`]
    ///
    /// [Google Sheets Docs: `spreadsheets.values.append`]: https://developers.google.com/sheets/api/reference/rest/v4/spreadsheets.values/append
    pub async fn append(&self, data: Vec<String>) -> Result<UpdateValuesResponse> {
        let request = self
            .request(
                Method::POST,
                &format!(
                    "spreadsheets/{}/values/{}:append",
                    self.sheet_id,
                    get_a1_notation(Some(0), None, Some(data.len()), None)
                ),
                ValueRange {
                    major_dimension: None,
                    values: Some(vec![data]),
                    range: None,
                },
                Some(vec![
                    ("valueInputOption", "USER_ENTERED"),
                    ("insertDataOption", "INSERT_ROWS"),
                ]),
            )
            .await;

        let res = self.client.execute(request).await.unwrap();

        match res.status() {
            StatusCode::OK => Ok(res.json().await.unwrap()),
            status_code => Err(ApiError::GoogleSheetsApi {
                status_code,
                body: res.text().await.unwrap(),
            }),
        }
    }

    /// Call the [`spreadsheets.values.batchUpdate` endpoint]:
    ///
    /// [`spreadsheets.values.batchUpdate` endpoint]: https://developers.google.com/sheets/api/reference/rest/v4/spreadsheets.values/batchUpdate
    #[allow(dead_code)]
    pub async fn batch_update(&self, data: Vec<Vec<String>>) -> Result<BatchUpdateValuesResponse> {
        let request = self
            .request(
                Method::POST,
                &format!("spreadsheets/{}/values:batchUpdate", self.sheet_id),
                &data,
                Some(vec![
                    ("valueInputOption", "USER_ENTERED"),
                    ("insertDataOption", "INSERT_ROWS"),
                ]),
            )
            .await;
        let res = self.client.execute(request).await.unwrap();
        match res.status() {
            StatusCode::OK => Ok(res.json().await.unwrap()),
            status_code => Err(ApiError::GoogleSheetsApi {
                status_code,
                body: res.text().await.unwrap(),
            }),
        }
    }

    pub async fn clear_sheet(&self) -> Result<UpdateValuesResponse> {
        let request = self
            .request(
                Method::POST,
                &format!("spreadsheets/{}/values/Sheet1:clear", self.sheet_id),
                EmptyBody {},
                None,
            )
            .await;

        let res = self.client.execute(request).await.unwrap();
        match res.status() {
            StatusCode::OK => Ok(res.json().await.unwrap()),
            s => Err(ApiError::GoogleSheetsApi {
                status_code: s,
                body: res.text().await.unwrap(),
            }),
        }
    }

    #[allow(dead_code)]
    pub async fn refresh_entire_sheet(
        &self,
        value: Vec<Vec<String>>,
    ) -> Result<UpdateValuesResponse> {
        self.clear_sheet().await?;
        self.update_values("A1", value).await
    }

    #[allow(dead_code)]
    pub async fn update_values(
        &self,
        range: &str,
        value: Vec<Vec<String>>,
    ) -> Result<UpdateValuesResponse> {
        let request = self
            .request(
                Method::PUT,
                &format!("spreadsheets/{}/values/{}", self.sheet_id, range),
                ValueRange {
                    major_dimension: Some(Dimension::ROWS),
                    range: Some(range.to_string()),
                    values: Some(value),
                },
                Some(vec![
                    ("valueInputOption", "USER_ENTERED"),
                    ("responseValueRenderOption", "FORMATTED_VALUE"),
                    ("responseDateTimeRenderOption", "FORMATTED_STRING"),
                ]),
            )
            .await;
        let res = self.client.execute(request).await.unwrap();
        match res.status() {
            StatusCode::OK => Ok(res.json().await.unwrap()),
            status_code => Err(ApiError::GoogleSheetsApi {
                status_code,
                body: res.text().await.unwrap(),
            }),
        }
    }
}

#[derive(Debug, Snafu)]
pub enum ApiError {
    #[snafu(display("Could not authenticate properly. {}: {}", meta, source))]
    AuthenticateError {
        source: std::io::Error,
        meta: String,
    },

    #[snafu(display("Client failed to build: {}", source))]
    ClientBuildFail { source: reqwest::Error },

    #[snafu(display("Token does not have proper scope {}: {}", scope, source))]
    TokenError { source: oauth::Error, scope: String },

    #[snafu(display("Error from Google Sheets API. {} {}", status_code, body))]
    GoogleSheetsApi {
        status_code: StatusCode,
        body: String,
    },
}

/// Use for any `POST` request that needs an empty body.
#[derive(Serialize)]
pub struct EmptyBody {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Dimension {
    /// Operates on the rows of a sheet.
    #[serde(rename = "ROWS")]
    ROWS,
    #[serde(rename = "COLUMNS")]
    /// Operates on the columns of a sheet.
    COLUMNS,
}

/// Data within the range of the spreadsheet.
///
/// See more at [Google Sheets Docs for `ValueRange`]
///
/// [Google Sheets Docs for `ValueRange]: https://developers.google.com/sheets/api/reference/rest/v4/spreadsheets.values#ValueRange
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct ValueRange {
    /// The range the values cover, in A1 notation.
    ///
    /// For output, this range indicates the entire requested range, even though the values will exclude trailing rows and columns.
    ///
    /// When appending values, this field represents the range to search for a table, after which values will be appended.
    pub range: Option<String>,
    /// The values
    pub values: Option<Vec<Vec<String>>>,
    /// The major dimension of the values.
    ///
    /// For output, if the spreadsheet data is: A1=1,B1=2,A2=3,B2=4, then requesting range=A1:B2,majorDimension=ROWS will return [[1,2],[3,4]], whereas requesting range=A1:B2,majorDimension=COLUMNS will return [[1,3],[2,4]].
    ///
    /// For input, with range=A1:B2,majorDimension=ROWS then [[1,2],[3,4]] will set A1=1,B1=2,A2=3,B2=4. With range=A1:B2,majorDimension=COLUMNS then [[1,2],[3,4]] will set A1=1,B1=3,A2=2,B2=4.
    ///
    /// When writing, if this field is not set, it defaults to "ROWS".
    #[serde(rename = "majorDimension")]
    pub major_dimension: Option<Dimension>,
}

/// The response returned from updating values.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UpdateValuesResponse {
    #[serde(rename = "spreadsheetId")]
    pub spreadsheet_id: Option<String>,

    #[serde(rename = "updatedColumns")]
    pub updated_columns: Option<i32>,

    #[serde(rename = "updatedRange")]
    pub updated_range: Option<String>,

    #[serde(rename = "updatedRows")]
    pub updated_rows: Option<i32>,

    #[serde(rename = "updatedData")]
    pub updated_data: Option<ValueRange>,

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

/// The response returned from Batch Updating Values
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct BatchUpdateValuesResponse {
    /// The spreadsheet the updates were applied to.
    #[serde(rename = "spreadsheetId")]
    pub spreadsheet_id: Option<String>,
    /// The total number of rows where at least one cell in the row was updated.
    #[serde(rename = "totalUpdatedRows")]
    pub total_updated_rows: Option<i32>,
    /// The total number of columns where at least one cell in the column was updated.
    #[serde(rename = "totalUpdatedColumns")]
    pub total_updated_columns: Option<i32>,
    /// The total number of cells updated.
    #[serde(rename = "totalUpdatedCells")]
    pub total_updated_cells: Option<i32>,
    /// The total number of sheets where at least one cell in the sheet was updated.
    #[serde(rename = "totalUpdatedSheets")]
    pub total_updated_sheets: Option<i32>,
    /// One `UpdateValuesResponse` per requested range, in the same order as the requests appeared.
    pub responses: Vec<UpdateValuesResponse>,
}
