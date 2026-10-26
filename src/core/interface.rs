#[derive(Clone, Debug)]
pub struct Bind {
    pub key_name: String,
    pub key_code: u32,
    pub mode: BindMode,
    pub target_state: BindTargetState,
}

impl Bind {
    pub fn new(key_name: impl Into<String>, key_code: u32) -> Self {
        Self {
            key_name: key_name.into(),
            key_code,
            mode: BindMode::Toggle,
            target_state: BindTargetState::On,
        }
    }

    pub fn with_mode(mut self, mode: BindMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_target_state(mut self, target_state: BindTargetState) -> Self {
        self.target_state = target_state;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindMode {
    Hold,
    Toggle,
}

impl BindMode {
    pub fn as_str(self) -> &'static str {
        match self {
            BindMode::Hold => "Hold",
            BindMode::Toggle => "Toggle",
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            BindMode::Hold => BindMode::Toggle,
            BindMode::Toggle => BindMode::Hold,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindTargetState {
    Off,
    On,
}

impl BindTargetState {
    pub fn as_str(self) -> &'static str {
        match self {
            BindTargetState::Off => "Off",
            BindTargetState::On => "On",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Category {
    Combat,
    Movement,
    Render,
    Player,
    World,
    Misc,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Combat => "Combat",
            Category::Movement => "Movement",
            Category::Render => "Render",
            Category::Player => "Player",
            Category::World => "World",
            Category::Misc => "Misc",
        }
    }

    pub const ALL: [Category; 6] = [
        Category::Combat,
        Category::Movement,
        Category::Render,
        Category::Player,
        Category::World,
        Category::Misc,
    ];
}

#[derive(Clone, Debug)]
pub struct CheckBox {
    pub name: String,
    pub value: bool,
}

impl CheckBox {
    pub fn new(name: impl Into<String>, value: bool) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Slider {
    pub name: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
}

impl Slider {
    pub fn new(name: impl Into<String>, value: f32, min: f32, max: f32) -> Self {
        Self {
            name: name.into(),
            value,
            min,
            max,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ColorPicker {
    pub name: String,
    pub value: [f32; 4],
}

impl ColorPicker {
    pub fn new(name: impl Into<String>, value: [f32; 4]) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ActionBind {
    pub name: String,
    pub bind: Bind,
}

impl ActionBind {
    pub fn new(name: impl Into<String>, key_name: impl Into<String>, key_code: u32) -> Self {
        Self {
            name: name.into(),
            bind: Bind::new(key_name, key_code),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Choice {
    pub name: String,
    pub options: Vec<String>,
    pub selected_index: usize,
}

impl Choice {
    pub fn new(name: impl Into<String>, options: Vec<String>, selected_index: usize) -> Self {
        let selected_index = selected_index.min(options.len().saturating_sub(1));
        Self {
            name: name.into(),
            options,
            selected_index,
        }
    }

    pub fn selected(&self) -> &str {
        self.options
            .get(self.selected_index)
            .map(String::as_str)
            .unwrap_or("")
    }
}

#[derive(Clone, Debug)]
pub enum Setting {
    CheckBox(CheckBox),
    Slider(Slider),
    ColorPicker(ColorPicker),
    ActionBind(ActionBind),
    Choice(Choice),
}

impl Setting {
    pub fn checkbox(name: impl Into<String>, value: bool) -> Self {
        Self::CheckBox(CheckBox::new(name, value))
    }

    pub fn slider(name: impl Into<String>, value: f32, min: f32, max: f32) -> Self {
        Self::Slider(Slider::new(name, value, min, max))
    }

    pub fn color_picker(name: impl Into<String>, value: [f32; 4]) -> Self {
        Self::ColorPicker(ColorPicker::new(name, value))
    }

    pub fn action_bind(
        name: impl Into<String>,
        key_name: impl Into<String>,
        key_code: u32,
    ) -> Self {
        Self::ActionBind(ActionBind::new(name, key_name, key_code))
    }

    pub fn choice(
        name: impl Into<String>,
        options: impl IntoIterator<Item = impl Into<String>>,
        selected_index: usize,
    ) -> Self {
        Self::Choice(Choice::new(
            name,
            options.into_iter().map(Into::into).collect(),
            selected_index,
        ))
    }
}

#[derive(Clone, Debug)]
pub struct Module {
    pub name: String,
    pub description: String,
    pub category: Category,
    pub enabled: bool,
    pub bind: Option<Bind>,
    pub settings: Vec<Setting>,
}

impl Module {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        category: Category,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            category,
            enabled: false,
            bind: Some(Bind::new("None", 0)),
            settings: Vec::new(),
        }
    }

    pub fn with_bind(mut self, bind: Bind) -> Self {
        self.bind = Some(bind);
        self
    }

    pub fn with_setting(mut self, setting: Setting) -> Self {
        self.settings.push(setting);
        self
    }
}
