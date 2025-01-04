use actix_cors::Cors;
use actix_web::{get, http::header, App, HttpResponse, HttpServer};
use rusqlite::{Connection, Result};
use serde::{Deserialize, Serialize};

use crate::types::*;

mod network;
mod types;

const DEBUG: bool = true;

#[derive(Debug, Serialize, Deserialize)]
struct MacroNutrients {
    protein: f64,
    carbs: f64,
    fats: f64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Ingredient {
    name: String,
    amount: f32,
    unit: String,
    macros: MacroNutrients,
}

// TODO: lol
#[derive(Debug, Serialize, Deserialize)]
struct LLMIngredient {
    name: String,
    amount: f32,
    unit: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Meal {
    id: u64,
    name: String,
    ingredients: Vec<Ingredient>,
    total_macros: MacroNutrients,
    timestamp: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct DailyLog {
    date: String,
    daily_goals: MacroNutrients,
    current_macros: MacroNutrients,
    meals: Vec<Meal>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Serving {
    amount: f64,
    unit: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct FoodItem {
    description: String,
    protein: f64,
    fat: f64,
    carbs: f64,
    serving: Serving,
}

#[derive(Debug, Serialize, Deserialize)]
struct MealResponse {
    recipe: String,
    macros: MacroNutrients,
}

// TODO: clearer terms for "ingredients" versus something in the database

fn prompt(system_prompt: &str, model: API) -> Result<Message, actix_web::Error> {
    match network::prompt(model, system_prompt, &vec![]) {
        Ok(msg) => Ok(msg),
        Err(_) => Err(actix_web::error::ErrorInternalServerError("Prompt failed")),
    }
}

fn initialize_db() -> Result<Connection> {
    let conn = Connection::open("foods.db")?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS macro_goals (
            protein REAL NOT NULL,
            carbs REAL NOT NULL,
            fats REAL NOT NULL
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS daily_logs (
            date TEXT PRIMARY KEY,
            current_protein REAL NOT NULL,
            current_carbs REAL NOT NULL,
            current_fats REAL NOT NULL
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS meals (
            id INTEGER PRIMARY KEY,
            date TEXT NOT NULL,
            name TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            total_protein REAL NOT NULL,
            total_carbs REAL NOT NULL,
            total_fats REAL NOT NULL,
            FOREIGN KEY(date) REFERENCES daily_logs(date)
        )",
        [],
    )?;

    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;

        CREATE VIRTUAL TABLE IF NOT EXISTS food_items USING fts5(
            description,
            protein UNINDEXED,
            fat UNINDEXED,
            carbs UNINDEXED,
            serving_amount UNINDEXED,
            serving_unit UNINDEXED
        );
    "#,
    )?;

    Ok(conn)
}

fn get_remaining_macros(conn: &Connection, date: &str) -> Result<MacroNutrients> {
    let goals: MacroNutrients = get_daily_macros(conn).unwrap();

    let current: MacroNutrients = conn.query_row(
        "SELECT COALESCE(current_protein, 0), COALESCE(current_carbs, 0), COALESCE(current_fats, 0) 
         FROM daily_logs WHERE date = ?1",
        [date],
        |row| {
            Ok(MacroNutrients {
                protein: row.get(0)?,
                carbs: row.get(1)?,
                fats: row.get(2)?,
            })
        },
    ).unwrap_or(MacroNutrients { protein: 0.0, carbs: 0.0, fats: 0.0 });

    Ok(MacroNutrients {
        protein: goals.protein - current.protein,
        carbs: goals.carbs - current.carbs,
        fats: goals.fats - current.fats,
    })
}

fn get_recent_meals(conn: &Connection, days: i32) -> Result<String> {
    let mut stmt = conn.prepare(
        "SELECT m.name, m.timestamp, m.total_protein, m.total_carbs, m.total_fats
         FROM meals m
         WHERE m.date >= date('now', ?1 || ' days')
         ORDER BY m.date DESC, m.timestamp DESC
         LIMIT 5",
    )?;

    let meals = stmt.query_map([days], |row| {
        Ok(format!(
            "- {} ({}): {}g protein, {}g carbs, {}g fats",
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, f32>(2)?,
            row.get::<_, f32>(3)?,
            row.get::<_, f32>(4)?,
        ))
    })?;

    Ok(meals
        .filter_map(Result::ok)
        .collect::<Vec<String>>()
        .join("\n"))
}

fn get_daily_macros(conn: &Connection) -> Result<MacroNutrients> {
    let mut stmt = conn.prepare("SELECT protein, carbs, fats FROM macro_goals LIMIT 1")?;

    Ok(stmt
        .query_row([], |row| {
            Ok(MacroNutrients {
                protein: row.get(0)?,
                carbs: row.get(1)?,
                fats: row.get(2)?,
            })
        })
        .unwrap_or(MacroNutrients {
            protein: 160.0,
            carbs: 350.0,
            fats: 100.0,
        }))
}

fn generate_meal_prompt(
    conn: &Connection,
    date: &str,
    meal_type: &str,
    height_cm: f32,
    weight_kg: f32,
    activity_level: &str,
) -> Result<String> {
    let remaining_macros = get_remaining_macros(conn, date)
        .map_err(|e| {
            format!(
                "Failed to retrieve remaining macro nutrients for {}: {}",
                date, e
            )
        })
        .unwrap();

    let meal_history = get_recent_meals(conn, 3)
        .map_err(|e| format!("Unable to fetch last 3 meals from history: {}", e))
        .unwrap();

    Ok(format!(
        "Create a meal recipe considering the following:

        Meal: {}
        Activity Level: {}

        Height: {} cm
        Weight: {} kg

        Remaining macros for the day:
        Protein: {}g
        Carbs: {}g
        Fats: {}g

        Recent meal history:
        {}

        Include detailed cooking instructions and a list of ingredients with specific measurements. Make sure all of your measurements are in _both_ grams and cups.
        Consider the meal history when making suggestions to ensure variety in the diet.
        Consider also how meal-prep friendly the meals are.
        Also remember: 3 meals in a day! A single meal _cannot_ be gargantuan. Be reasonable--no more than a half of the daily requirements.",
        meal_type,
        activity_level,
        height_cm,
        weight_kg,
        remaining_macros.protein,
        remaining_macros.carbs,
        remaining_macros.fats,
        meal_history
    ))
}

fn search_foods(conn: &Connection, query: &str) -> Result<Option<FoodItem>> {
    let mut stmt = conn.prepare(
        "
            SELECT description, protein, fat, carbs, serving_amount, serving_unit
            FROM food_items 
            WHERE description MATCH ?
            ORDER BY rank
            LIMIT 1
        ",
    )?;

    let mut result = stmt.query_map([query], |row| {
        Ok(FoodItem {
            description: row.get(0)?,
            protein: row.get(1)?,
            fat: row.get(2)?,
            carbs: row.get(3)?,
            serving: Serving {
                amount: row.get(4)?,
                unit: row.get(5)?,
            },
        })
    })?;

    Ok(result.next().transpose()?)
}

fn adjust_recipe(
    conn: &Connection,
    recipe: &str,
    macros: &MacroNutrients,
    ingredients: &Vec<FoodItem>,
) -> Result<String, std::io::Error> {
    // TODO: obviously the date should reflect today's
    let date = chrono::Local::now().date().format("%Y-%m-%d").to_string();
    let remaining_macros = get_remaining_macros(conn, &date)
        .map_err(|e| {
            format!(
                "Failed to retrieve remaining macro nutrients for {}: {}",
                date, e
            )
        })
        .unwrap();

    // all the ingredients need their serving sizes adjusted by the factor
    // required to get the macros to the desired target
    //
    // i think since we're dividing gram by gram (g/g), the units cancel out?
    // and we can take the resulting value (factors) and apply that to each
    // ingredient's serving size to adjust the macros accordingly
    let macro_goals = get_daily_macros(conn).unwrap();

    let target = MacroNutrients {
        protein: f64::min(
            macro_goals.protein / 3.0,
            f64::min(remaining_macros.protein, macros.protein),
        ),
        carbs: f64::min(
            macro_goals.carbs / 3.0,
            f64::min(remaining_macros.carbs, macros.carbs),
        ),
        fats: f64::min(
            macro_goals.fats / 3.0,
            f64::min(remaining_macros.fats, macros.fats),
        ),
    };

    let factors = MacroNutrients {
        protein: target.protein / macros.protein,
        carbs: target.carbs / macros.carbs,
        fats: target.fats / macros.fats,
    };

    println!("target: {:?}", target);
    println!("factors: {:?}", factors);

    let ingredients = ingredients.iter().map(|i| {
        let mut ing = i.clone();
        let scaling_factor = if ing.protein >= ing.carbs && ing.protein >= ing.fat {
            factors.protein
        } else if ing.carbs >= ing.protein && ing.carbs >= ing.fat {
            factors.carbs
        } else {
            factors.fats
        };

        ing.serving.amount *= scaling_factor;

        ing
    });

    println!("ingredients: {:?}", ingredients);

    let system_prompt = format!("
        I have adjusted some of the amounts of each ingredient in the recipe. Please rewrite to account for these new values:
            {:?}

        The recipe:
            {}

        Notes:
            - not all ingredients are included in that list--please still adjust accordingly
            - _do not_ acknowledge that your message is in response to mine--simply reply with only the rewritten recipe
    ", ingredients, recipe);

    let rewritten = prompt(&system_prompt, API::OpenAI(OpenAIModel::GPT4o)).unwrap();

    Ok(rewritten.content)
}

fn calculate_macros(conn: &Connection, recipe: &str) -> (MacroNutrients, Vec<FoodItem>) {
    let ingredients: Vec<LLMIngredient> = serde_json::from_str(&prompt(
        &format!(
            r#"
                Generate a JSON array--the response must _only_ be JSON, no markdown!--of ingredient names from the given recipe in the given format. Ignore spices or other nutritionally negligible ingredients--this is important! you must be tasteful in what is important nutritionally, think macros! Keep names simple--nothing more than two words. Also--_nothing plural_.

                Format:
                    [
                        {{
                            "name": "string",
                            "amount": "number",
                            "unit": "string"
                        }}
                    ]

                Recipe:
                    {}

                ---
                remember, no markdown!
            "#,
            recipe
        ),
        API::OpenAI(OpenAIModel::GPT4oMini)
    ).unwrap().content.replace("\\n", "\n")).unwrap();

    let mut macros = MacroNutrients {
        protein: 0.0,
        carbs: 0.0,
        fats: 0.0,
    };

    let mut retrieved_ingredients = Vec::new();
    for i in ingredients.iter() {
        let name = i
            .name
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>();

        let lookup = search_foods(&conn, &name).unwrap();
        if lookup.is_none() {
            println!("couldn't find ingredient {}; ignoring...", i.name);
            continue;
        }

        let mut lookup = lookup.unwrap();

        let factor = lookup.serving.amount / i.amount as f64;

        lookup.serving.amount = i.amount as f64;
        lookup.protein /= factor;
        lookup.carbs /= factor;
        lookup.fat /= factor;

        macros.protein += lookup.protein;
        macros.carbs += lookup.carbs;
        macros.fats += lookup.fat;

        retrieved_ingredients.push(lookup.clone());
    }

    (macros, retrieved_ingredients)
}

fn quality_gate(conn: &Connection, recipe: &str) -> bool {
    let (macros, _) = calculate_macros(&conn, recipe);
    let system_prompt = format!(
        "
        Are the portions in this recipe reasonable?

        Here is the recipe:
            {}

        Here are the calculated macros:
            {:?}

        Reply with only \"yes\" or \"no\"--nothing else!
    ",
        recipe, macros
    );

    let response = prompt(&system_prompt, API::OpenAI(OpenAIModel::GPT4o)).unwrap();

    &response.content == "yes"
}

#[get("/generate-meal")]
async fn generate_meal() -> Result<HttpResponse, actix_web::Error> {
    if DEBUG {
        let meal: MealResponse = serde_json::from_str(
            &std::fs::read_to_string("/home/joey/rust/chamber/pierre/src/testing.json").unwrap(),
        )
        .unwrap();

        Ok(HttpResponse::Ok().json(meal))
    } else {
        let conn = initialize_db()
            .map_err(|e| {
                eprintln!("Database error: {}", e);
                actix_web::error::ErrorInternalServerError("Database error")
            })
            .unwrap();

        let system_prompt = generate_meal_prompt(
            &conn,
            &chrono::Local::now().date().format("%Y-%m-%d").to_string(),
            "dinner",
            175.0,
            70.0,
            "active",
        )
        .unwrap();

        let recipe = prompt(&system_prompt, API::OpenAI(OpenAIModel::GPT4o)).unwrap();

        let (macros, ingredients) = calculate_macros(&conn, &recipe.content);

        let adjusted = adjust_recipe(&conn, &recipe.content, &macros, &ingredients).unwrap();
        let adjusted = adjusted.replace("\\n", "\n");

        let (macros, _) = calculate_macros(&conn, &adjusted);
        Ok(HttpResponse::Ok().json(MealResponse {
            recipe: adjusted,
            macros,
        }))
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    if let Err(e) = initialize_db() {
        eprintln!("Failed to initialize database: {}", e);
        return Ok(());
    }

    HttpServer::new(|| {
        let cors = Cors::default()
            .allowed_origin("http://localhost:5173")
            .allowed_methods(vec!["GET", "POST"])
            .allowed_headers(vec![header::AUTHORIZATION, header::ACCEPT])
            .allowed_header(header::CONTENT_TYPE)
            .max_age(3600);

        App::new().wrap(cors).service(generate_meal)
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
