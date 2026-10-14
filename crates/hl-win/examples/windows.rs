//! Lists the windows «Окно» capture would offer, front to back:
//! `cargo run -p hl-win --example windows`

fn main() {
    for (h, (l, t, r, b)) in hl_win::window::visible_windows() {
        println!("{:>10} {:>5},{:<5} {:>5}x{:<5} {:<22} {}", h.0, l, t, r - l, b - t, hl_win::window::exe_name(h), hl_win::window::title(h));
    }
}
