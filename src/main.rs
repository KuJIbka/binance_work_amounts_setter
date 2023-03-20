extern crate reqwest;

use serde::{Deserialize, Serialize};
use serde_json::{Value};
use xml_doc::Document;
use std::collections::HashMap;
// use std::ffi::OsStr;
use std::io::{self, Write};
use std::fs::{self, OpenOptions};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
struct SymbolPrice {
  symbol: String,
  price: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SymbolExchangeInfo {
  symbol: String,
  filters: Value
}

#[derive(Debug, Serialize, Deserialize)]
struct SymbolEchangeRatesResponse {
  symbols: Vec<SymbolExchangeInfo>,
}


fn main() {
    const DIR_PATH: &str = ".\\Data";
    let mvs_path = DIR_PATH.to_owned() + "\\MVS";
    let mvs_path_backup = mvs_path.clone() + "__backup";
    let mut any = String::new();
    // let xml_ext = Some("xml");

    // Check poistion
    if !Path::new(&mvs_path).is_dir() {
        println!("Убедитесь, что верно располижили файл. Необходимо положить в корневую папку CScalp");

        let _skip = io::stdin().read_line(&mut any);
        return;
    }

    // Make a backup
    if Path::new(&mvs_path_backup).is_dir() {
        fs::remove_dir_all(&mvs_path_backup).expect("Can't not delete backup folder");
    }
    copy_dir_all(&mvs_path, &mvs_path_backup).expect("can't create backup folder");

    let usdt_data = get_user_input_data();
    let main_amount = usdt_data.0;
    let multipliers = usdt_data.1;

    let symbols_step_sizes = get_symbol_step_sizes_fut();
    let symbolds_prices = get_symbol_prices_fut();

    let lots = calc_all_lots(symbolds_prices, symbols_step_sizes, main_amount as f64, multipliers);


    for file in fs::read_dir(mvs_path).unwrap() {
        let dir_entry = file.unwrap();
        // if dir_entry.path().extension().and_then(OsStr::to_str).ne(&xml_ext) {
        //     continue;
        // }
        let file_name_str = dir_entry.file_name().into_string().unwrap();
        let mut file_name_parts = file_name_str.split('.');
        let burse = file_name_parts.nth(0).unwrap();
        let spot_or_fut = match file_name_parts.nth(0) {
            Some(sof) => sof,
            None => ""
        };

        // Skip all spot and not binance
        if burse != "BINAD" || spot_or_fut != "CCUR_FUT" {
            continue;
        }

        let symbol_from_file_name = file_name_parts.rev().nth(1)
            .unwrap().split('_').next().unwrap()
        ;
        print!("Update for symbol {}", symbol_from_file_name);
        if lots.contains_key(symbol_from_file_name) {
            update_work_amounts(dir_entry.path().to_str().unwrap(), &lots[symbol_from_file_name]);
            println!(" - OK");
        } else {
            println!("- price not found for {}", symbol_from_file_name);
        }
    }
    
    println!("Нажимте Enter для выхода");
    
    let _skip = io::stdin().read_line(&mut any);
}

fn get_user_input_data() -> (u32, Vec<f64>) {
    let mut main_amount_str = String::new();
    println!("=========================================");
    println!("Перед автоматической установкой объёмов ОБЯЗАТЕЛЬНО сделайте бекап текущих настроек!!!"); 
    println!("Для настройки объёмов, Вам нужно будет ввести сначала базовую сумму, потом 4 множителя.");
    println!("Например, базовая сумма 100 USDT, множители \"2 3 4 5\", а сумма монеты 1000 usdt");
    println!("В этом случае объёмы будут 0.1 (1000 / 100), 0.2 (0.1 * 2), 0.3 (0.1 * 3), 0.4 (0.1 * 4), 0.5 (0.1 * 5)");
    println!("=========================================");
    println!("Введите базовую сумму в USDT:");
    io::stdin().read_line(&mut main_amount_str).unwrap();
    let main_amount: u32 = main_amount_str.trim().parse().expect("Введите число");

    println!("Ввдите 5 чисел множителей основной суммы для объёмов (по умолчанию \"1 2 3 4 5\"):");
    let mut multipliers_str = String::new();
    io::stdin().read_line(&mut multipliers_str).unwrap();
    if multipliers_str.trim() == "" {
        multipliers_str = String::from("1 2 3 4 5");
    }
    let multipliers = multipliers_str.split(' ').filter_map(|s| s.trim().parse::<f64>().ok()).collect::<Vec<_>>();
    println!("Ваша базовая сумма {} USDT и множители \"{:?}\"", main_amount, multipliers);

    (main_amount, multipliers)
}

fn update_work_amounts(path: &str, lots: &[f64; 5]) {
    let mut doc = Document::parse_file(path).unwrap();
    let trading_el = doc.root_element().unwrap().find(&doc, "TRADING").unwrap();
    let xml_keys = ["First_WorkAmount", "Second_WorkAmount", "Third_WorkAmount", "Fourth_WorkAmount", "Fifth_WorkAmount"];

    for (i, key) in xml_keys.iter().enumerate() {
        let lot_value = lots[i];
        let workamount_element = trading_el.find(&doc, key).unwrap();
        workamount_element.set_attribute(&mut doc, "Value", lot_value.to_string());
        workamount_element.attribute(&doc, "Value");
    }

    let mut xml_file = OpenOptions::new().write(true).truncate(true).open(path).unwrap();
    let r = xml_file.write(doc.write_str().unwrap().as_bytes());
    
    match r {
        Err(e) => println!("{:?}", e),
        _ => {}
    }
}

fn get_symbol_prices_fut() -> HashMap<String, f64> {
    let body = reqwest::blocking::get("https://www.binance.com/fapi/v1/ticker/price")
      .unwrap().text().expect("can't get binance prices");
    let prices_data: Vec<SymbolPrice> = serde_json::from_str(&body).unwrap();
    let mut symbols_prices: HashMap<String, f64> = HashMap::new();
    
    for symbol_price_data in prices_data {
        let symbol_price: f64 = fix_numbers(symbol_price_data.price.as_str());
        symbols_prices.insert(symbol_price_data.symbol.to_string(), symbol_price);
    }

    symbols_prices
}

fn get_symbol_step_sizes_fut() -> HashMap<String, f64> {
    let body = reqwest::blocking::get("https://www.binance.com/fapi/v1/exchangeInfo")
        .unwrap().text().expect("error when load exchange info");
    let exchange_info_data: SymbolEchangeRatesResponse = serde_json::from_str(&body).unwrap();
    let mut symbols_step_sizes: HashMap<String, f64> = HashMap::new();
    for exchange_info in exchange_info_data.symbols {
        let step_size_price: f64 = fix_numbers(exchange_info.filters[1]["stepSize"].as_str().unwrap());
        symbols_step_sizes.insert(exchange_info.symbol.to_string(), step_size_price);
    }

    symbols_step_sizes
}

fn fix_numbers(number_as_str: &str) -> f64 {
    let trimmed_str = number_as_str.trim_end_matches('0').trim_end_matches('.').to_string();
    trimmed_str.parse().unwrap()
}

fn calc_all_lots(prices: HashMap<String, f64>, step_sizes: HashMap<String, f64>, main_amount: f64, multipliers: Vec<f64>) -> HashMap<String, [f64; 5]> {
    let mut result: HashMap<String, [f64; 5]> = HashMap::new();
    for (symbol, price) in prices {
        if !step_sizes.contains_key(&symbol) {
            continue;
        }
        let precision = get_precision_from_step(step_sizes[&symbol]);
        let mut main_lot: f64 = main_amount as f64 / price;
        main_lot = precision_round(main_lot, precision);
        main_lot = if main_lot < step_sizes[&symbol] { step_sizes[&symbol] } else { main_lot };

        let mut lots = [0.0, 0.0, 0.0, 0.0, 0.0];
        for (i, multiply) in multipliers.iter().enumerate() {
            let mut next_lot = main_lot * multiply;
            next_lot = precision_round(next_lot, precision);
            lots[i] = next_lot;
        }

        result.insert(symbol.clone(), lots);
    }

    result
}

fn precision_round(amount: f64, precision: u32) -> f64 {
    (amount * 10_i32.pow(precision) as f64).round() / 10_i32.pow(precision) as f64
}

fn get_precision_from_step(step: f64) -> u32 {
    let mut precision = 0;
    let mut step_mut = step;
    let mut max_try = 100;
    loop {
        if step_mut == 1_f64 {
            break;
        }
        precision += 1;
        step_mut *= 10_f64; 

        max_try -= 1;
        if max_try == 0 {
            println!("===== SOMETHING WRONG {}", step);
            break;
        }
    }

    precision
}

fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> io::Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}