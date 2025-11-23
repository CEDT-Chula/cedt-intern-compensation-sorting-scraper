use ::futures::future::join_all;
use polars::frame::DataFrame;
use polars::prelude::{Column, IntoLazy};
use polars::prelude::{JsonWriter, SerWriter, SortMultipleOptions};
use reqwest::header::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use std::io::{self, Write as _};
use std::{fs, path::PathBuf};

fn get_opening_listing_url(semester: u32) -> String {
  return format!("https://cedtintern.cp.eng.chula.ac.th/api/sessions/{}/openings?search=&limit=20&onlyBookmarked=false&onlyAvailablePositions=false",semester);
}
fn get_opening_url(semester: u32) -> String {
  return format!(
    "https://cedtintern.cp.eng.chula.ac.th/api/sessions/{}/openings",
    semester
  );
}
const FIRST_CEDT_YEAR: u32 = 2023;

fn load_cookie() -> String {
  fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".cookie")).unwrap_or_else(
    |_| panic!("there is no cookie, please insert cookie at 'project root/.cookie'"),
  )
}

async fn get_opening_list(reqwest_client: &reqwest::Client, page: u32, semester: u32) -> String {
  let url = format!(
      "{}&page={}",
      get_opening_listing_url(semester),
      page
    );
  println!("Fetching {}",url);
  let response = reqwest_client
    .get(url)
    .send()
    .await
    .expect(&format!("failed to get response to page {}", page))
    .text()
    .await
    .expect(&format!(
      "failed to get text from response to page {}",
      page
    ));

  return response;
}

fn get_opening_ids(opening_response: &OpeningsListResponse) -> Vec<u32> {
  return opening_response
    .items
    .iter()
    .map(|item| item.opening_id)
    .collect();
}

fn read_response_to_json(content: &str) -> OpeningsListResponse {
  return serde_json::from_str(&content).unwrap();
}

async fn fetch_openings_ids(
  reqwest_client: &reqwest::Client,
  page: u32,
  semester: u32,
) -> Vec<u32> {
  let response = get_opening_list(&reqwest_client, page, semester).await;
  let response_json = read_response_to_json(&response);
  return get_opening_ids(&response_json);
}

async fn get_all_opening_ids(reqwest_client: &reqwest::Client, semester: u32) -> Vec<u32> {
  let first_page = get_opening_list(&reqwest_client, 1, semester).await;
  let first_page_json = read_response_to_json(&first_page);

  let mut opening_ids = get_opening_ids(&first_page_json);

  let mut tasks = vec![];
  for page in 2..=first_page_json.meta.total_page {
    tasks.push(fetch_openings_ids(&reqwest_client, page, semester));
  }
  opening_ids = [opening_ids, join_all(tasks).await.concat()].concat();

  return opening_ids;
}

async fn get_opening(
  reqwest_client: &reqwest::Client,
  opening_id: u32,
  semester: u32,
) -> OpeningResponse {
  let response = reqwest_client
    .get(format!("{}/{}", get_opening_url(semester), opening_id))
    .send()
    .await
    .expect(&format!("failed to get response to page {}", opening_id))
    .text()
    .await
    .expect(&format!(
      "failed to get text from response to page {}",
      opening_id
    ));

  // println!("{}",response);

  let response_json: OpeningResponse = serde_json::from_str(&response).unwrap();
  return response_json;
}

async fn get_opening_from_list(
  reqwest_client: &reqwest::Client,
  opening_ids: Vec<u32>,
  semester: u32,
) -> Vec<OpeningResponse> {
  let mut tasks = vec![];
  for opening_id in opening_ids {
    tasks.push(get_opening(reqwest_client, opening_id, semester))
  }
  let result = join_all(tasks).await;
  return result;
}

fn prepare_dataframe(openings: Vec<OpeningResponse>) -> DataFrame {
  let mut companies_en: Vec<String> = vec![];
  let mut companies_th: Vec<String> = vec![];
  let mut job_titles: Vec<String> = vec![];
  let mut descriptions: Vec<String> = vec![];
  let mut opening_ids: Vec<u32> = vec![];

  for opening in openings {
    companies_en.push(opening.company.company_name_en);
    companies_th.push(opening.company.company_name_th);
    job_titles.push(opening.title);
    descriptions.push(opening.description.unwrap_or("-".into()));
    opening_ids.push(opening.opening_id);
  }

  DataFrame::new(vec![
    Column::new("company_en".into(), companies_en),
    Column::new("company_th".into(), companies_th),
    Column::new("job_title".into(), job_titles),
    Column::new("description".into(), descriptions),
    Column::new("opening_id".into(), opening_ids),
  ])
  .unwrap()
}

#[tokio::main]
async fn main() {
  let mut input = String::new();

  print!("Please enter year: ");
  io::stdout().flush().expect("Failed to flush stdout");
  io::stdin()
    .read_line(&mut input)
    .expect("Failed to read line");

  let year = input.trim().parse::<u32>().expect("Invalid year");

  let semester: u32 = (year - FIRST_CEDT_YEAR) * 2 + 1;

  println!("Fetching session/semester {}...",semester);

  let cookie = load_cookie();

  let mut headers = HeaderMap::new();
  headers.insert(
    reqwest::header::COOKIE,
    HeaderValue::from_str(&cookie).expect("cannot parse cookie into HeaderValue"),
  );

  let reqwest_client = reqwest::Client::builder()
    .default_headers(headers)
    .build()
    .unwrap();

  let opening_ids = get_all_opening_ids(&reqwest_client, semester).await;

  let openings = get_opening_from_list(&reqwest_client, opening_ids, semester).await;

  let mut compensation_type = openings
    .iter()
    .map(|obj| {
      obj
        .compensation_type
        .as_ref()
        .map_or(None, |obj| Some(obj.compensation_type.clone()))
    })
    .collect::<Vec<Option<String>>>();

  let mut compensation_amount = openings
    .iter()
    .map(|obj| obj.compensation_amount)
    .collect::<Vec<Option<u32>>>();

  for i in 0..openings.len() {
    if let Some(compensation_amount) = &mut compensation_amount[i] {
      if let Some(compensation_type) = &mut compensation_type[i] {
        if compensation_type == "บาท/วัน" {
          *compensation_amount *= 20;
          *compensation_type = "บาท/เดือน".into();
        }
      }
    }
  }

  // let mut df = array_of_struct_to_dataframe!(openings, [description, title, opening_id]).unwrap();

  let mut df = prepare_dataframe(openings);

  df.with_column(Column::new("compensation_type".into(), compensation_type))
    .unwrap();

  df.with_column(Column::new(
    "compensation_amount".into(),
    compensation_amount,
  ))
  .unwrap();

  let mut df = df
    .lazy()
    .drop_nulls(Some(vec!["compensation_amount".into()]))
    .sort(
      ["compensation_amount"],
      SortMultipleOptions {
        descending: vec![true],
        nulls_last: vec![false],
        multithreaded: true,
        maintain_order: false,
        limit: None,
      },
    )
    .collect()
    .unwrap();

  df.with_column(Column::new(
    "link".into(),
    df["opening_id"]
      .u32()
      .unwrap()
      .iter()
      .map(|opening_id| {
        format!(
          "https://cedtintern.cp.eng.chula.ac.th/opening/{}/session/{}",
          opening_id.unwrap(),
          semester
        )
      })
      .collect::<Vec<String>>(),
  ))
  .unwrap();

  let df = df.drop("opening_id").unwrap();
  let mut df = df.head(Some(100));

  let mut buffer: Vec<u8> = vec![];
  let mut writer = JsonWriter::new(&mut buffer).with_json_format(polars::prelude::JsonFormat::Json);
  writer.finish(&mut df).unwrap();

  let json_string = String::from_utf8(buffer).unwrap();
  let pretty_json = jsonxf::pretty_print(&json_string).unwrap();

  std::fs::write(
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("result.json"),
    pretty_json,
  )
  .unwrap();

  println!("Done! saved to {}",PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("result.json").display());
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpeningsListResponseItem {
  opening_id: u32,
  title: String,
  quota: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpeningsListResponseMeta {
  total_item: u32,
  items_per_page: u32,
  total_page: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpeningsListResponse {
  items: Vec<OpeningsListResponseItem>,
  meta: OpeningsListResponseMeta,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompanyInfo {
  company_name_th: String,
  company_name_en: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpeningResponse {
  opening_id: u32,
  title: String,
  description: Option<String>,
  compensation_amount: Option<u32>,
  compensation_type: Option<CompensationType>,
  working_condition: Option<String>,
  company: CompanyInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompensationType {
  compensation_type_id: u32,
  compensation_type: String,
}
