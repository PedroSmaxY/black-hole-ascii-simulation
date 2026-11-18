use black_hole_ascii::app::App;
use black_hole_ascii::render::buffer::RenderMode;
use black_hole_ascii::render::hud::Lang;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = if args.iter().any(|a| a == "--ascii") {
        RenderMode::Ascii
    } else {
        RenderMode::HalfBlock
    };
    let lang = Lang::detect(&args);

    let mut app = App::new(mode, lang);
    app.run();
}
