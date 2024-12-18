# cedt-intern-compensation-sorting-scraper
a web scraper to scrape cedtintern.cp.eng.chula.ac.th then sort internship by compensation amount using Polars

# Too lazy to run
for CEDT01 in 2024: first 100 internships with most compensation amount(assuming 1 month = 20 working days) is in [here](https://pastebin.com/jDTS6Fvk)

# How to use?
- copy http-only cookie named "athena_session" (for most browsers it should be at `dev tool(f12)>storage>cookies`) from cedtintern.cp.eng.chula.ac.th into project_directory/.cookie file
  it should look like `athena_session:...`
- run `cargo run` in project directory
- result will be in project_directory/result.json

# Why rust with Polars? Why not python with pandas? What the hell is even Polars
  I just feel like coding rust. Polars is almost like Pandas but with rust support
