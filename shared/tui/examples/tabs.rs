use std::time::Duration;

use psyche_tui::{init_logging, start_render_loop, CustomWidget, TabbedWidget};
use rand::{seq::SliceRandom, Rng};
use ratatui::widgets::{Paragraph, Widget};
use tracing::{error, info, warn};

mod minimal;
use minimal::MinimalWidget;

#[derive(Default)]
struct SillyDogWidget;
impl CustomWidget for SillyDogWidget {
    type Data = String;

    fn render(
        &mut self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &Self::Data,
    ) {
        Paragraph::new(format!(
            r#"
      / \__
     (    @\___
    /          O
   /    (_____/    {state}
  /_____/    U   "#,
        ))
        .render(area, buf);
    }
}

type Widgets = TabbedWidget<(SillyDogWidget, MinimalWidget)>;

const BARKS: [&str; 5] = ["bork", "woof", "boof", "bark", "hello im a dog"];

fn main() -> anyhow::Result<()> {
    init_logging(psyche_tui::LogOutput::TUI);

    info!("foo");
    warn!("bar");
    error!("baz");

    let tx = start_render_loop(Widgets::new(
        Default::default(),
        vec!["Silly Dog".to_string(), "State Example".to_string()],
    ))?;

    loop {
        let mut rng = rand::thread_rng();
        let random_num = rng.gen::<u64>();
        let bark = BARKS.choose(&mut rng).unwrap().to_string();
        let states = (bark, random_num);
        tx.send(states).expect("sending works!");
        std::thread::sleep(Duration::from_secs(2));
    }
}
