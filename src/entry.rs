use chrono::{DateTime, Local};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: String,
    pub title: Option<String>,
    pub created_at: DateTime<Local>,
    pub updated_at: DateTime<Local>,
    pub tags: Vec<String>,
    pub mood: Option<Mood>,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mood {
    Great,
    Good,
    Okay,
    Bad,
    Awful,
}

impl std::fmt::Display for Mood {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mood::Great => write!(f, "great"),
            Mood::Good => write!(f, "good"),
            Mood::Okay => write!(f, "okay"),
            Mood::Bad => write!(f, "bad"),
            Mood::Awful => write!(f, "awful"),
        }
    }
}

impl std::str::FromStr for Mood {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> anyhow::Result<Self> {
        match s.to_lowercase().as_str() {
            "great" => Ok(Mood::Great),
            "good" => Ok(Mood::Good),
            "okay" | "ok" => Ok(Mood::Okay),
            "bad" => Ok(Mood::Bad),
            "awful" => Ok(Mood::Awful),
            _ => anyhow::bail!("Invalid mood: {}. Use: great, good, okay, bad, awful", s),
        }
    }
}

impl Entry {
    pub fn new(title: Option<String>, tags: Vec<String>, mood: Option<Mood>) -> Self {
        let now = Local::now();
        Self {
            id: Uuid::new_v4().to_string(),
            title,
            created_at: now,
            updated_at: now,
            tags,
            mood,
            body: String::new(),
        }
    }

    pub fn word_count(&self) -> usize {
        self.body.split_whitespace().count()
    }
}
