use andrew::{
    shapes::rectangle,
    text::{self, fontconfig},
    Canvas,
};

use smithay_client_toolkit::{
    default_environment,
    environment::SimpleGlobal,
    init_default_environment,
    output::{with_output_info, OutputInfo},
    reexports::{
        calloop,
        client::protocol::{
            wl_keyboard, wl_output,
            wl_pointer::{self, ButtonState},
            wl_shm, wl_surface,
        },
        protocols::wlr::unstable::layer_shell::v1::client::{
            zwlr_layer_shell_v1, zwlr_layer_surface_v1,
        },
    },
    seat::{
        self,
        keyboard::{map_keyboard_repeat, RepeatKind},
    },
    shm::DoubleMemPool,
    window::{self, ConceptFrame},
    WaylandSource,
};

use std::{
    cell::{Cell, RefCell},
    env,
    io::{self, Read, Seek, SeekFrom, Write},
    process::{self, Command},
    rc::Rc,
};

mod args;
use args::{Args, WindowType};

const FONT_COLOR: [u8; 4] = [255, 255, 255, 255];

default_environment!(Env,
    fields = [
        layer_shell: SimpleGlobal<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    ],
    singles = [
        zwlr_layer_shell_v1::ZwlrLayerShellV1 => layer_shell
    ],
);

default_environment!(NormalWindowEnv, desktop);

#[derive(PartialEq, Copy, Clone)]
enum RenderEvent {
    Configure { width: u32, height: u32 },
    Closed,
}

struct Surface {
    args: Args,
    next_render_event: Rc<Cell<Option<RenderEvent>>>,
    pools: DoubleMemPool,
    dimensions: (u32, u32),
    /// X, Y coordinates of current cursor position
    pointer_location: Option<(f64, f64)>,
    /// User requested exit
    should_exit: bool,
    click_targets: Vec<ClickTarget>,
    font_data: Vec<u8>,
}

struct ClickTarget {
    position: (usize, usize),
    size: (usize, usize),
    handler: ClickHandler,
}

#[derive(Clone)]
enum ClickHandler {
    /// Request to exit
    Exit,
    /// Run command
    RunCommand(String),
}

impl Surface {
    fn new(
        args: Args,
        pools: DoubleMemPool,
        next_render_event: Rc<Cell<Option<RenderEvent>>>,
    ) -> Self {
        let mut font_data = Vec::new();
        std::fs::File::open(
            &fontconfig::FontConfig::new()
                .expect("failed to find font config file")
                // .get_regular_family_fonts("monospace")
                .get_fonts()
                .unwrap()
                .pop()
                .expect("should find at least one font"),
        )
        .unwrap()
        .read_to_end(&mut font_data)
        .unwrap();

        Self {
            args,
            next_render_event,
            pools,
            dimensions: (0, 0),
            pointer_location: None,
            should_exit: false,
            click_targets: vec![],
            font_data,
        }
    }

    /// Handles any events that have occurred since the last call, redrawing if needed.
    /// Returns true if the surface should be dropped.
    fn handle_events(&mut self, surface: &wl_surface::WlSurface) -> bool {
        match self.next_render_event.take() {
            Some(RenderEvent::Closed) => true,
            Some(RenderEvent::Configure { width, height }) => {
                self.dimensions = (width, height);
                self.draw(surface);
                false
            }
            None => self.should_exit,
        }
    }

    fn handle_pointer_event(&mut self, event: &wl_pointer::Event) {
        match event {
            wl_pointer::Event::Enter {
                surface_x,
                surface_y,
                ..
            }
            | wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => self.pointer_location = Some((*surface_x, *surface_y)),
            wl_pointer::Event::Button {
                state: ButtonState::Pressed,
                ..
            } => {
                let mut matching_click_handler = None;
                for click_target in &self.click_targets {
                    if let Some(click_position) = self.pointer_location {
                        if let Some(handler) = click_target.process_click(click_position) {
                            matching_click_handler = Some(handler);
                        }
                    }
                }

                match matching_click_handler {
                    Some(ClickHandler::Exit) => self.should_exit = true,
                    Some(ClickHandler::RunCommand(cmd)) => {
                        match Command::new("/bin/sh").arg("-c").arg(cmd).spawn() {
                            Ok(_) => (),
                            Err(e) => eprintln!("{:?}", e),
                        }
                    }
                    None => {}
                }
            }
            _ => {}
        }
    }

    fn draw(&mut self, surface: &wl_surface::WlSurface) {
        let pool = match self.pools.pool() {
            Some(pool) => pool,
            None => return,
        };

        let stride = 4 * self.dimensions.0 as i32;
        let width = self.dimensions.0 as i32;
        let height = self.dimensions.1 as i32;

        let vertical_padding = 2;
        let horizontal_padding = 10;
        let max_text_size = 16.;
        let text_h = {
            let h = height as f32 / 2.;
            if h > max_text_size {
                max_text_size
            } else {
                h
            }
        };

        let text_hh = text_h / 2.;

        // First make sure the pool is the right size
        pool.resize((stride * height) as usize).unwrap();

        let mut buf: Vec<u8> = vec![255; (4 * width * height) as usize];
        let mut canvas = andrew::Canvas::new(
            &mut buf,
            width as usize,
            height as usize,
            4 * width as usize,
            andrew::Endian::native(),
        );

        // Draw background
        let block = rectangle::Rectangle::new(
            (0, 0),
            (width as usize, height as usize),
            None,
            Some([255, 200, 0, 0]),
        );
        canvas.draw(&block);

        // Draw buttons
        let mut right_most_pixel = width as usize;

        let mut draw_button = move |text: String, font_data: &[u8], canvas: &mut Canvas| {
            let mut text = text::Text::new((0, 0), FONT_COLOR, font_data, text_h, 1.0, text);
            let text_width = text.get_width();
            let button_width = text_width + 2 * horizontal_padding;
            let block_height = height as usize - vertical_padding * 2;
            let block_pos = (
                right_most_pixel as usize - button_width - horizontal_padding,
                vertical_padding,
            );
            let text_pos = (
                block_pos.0 + horizontal_padding,
                ((block_height as f32 - text_h) / 2.) as usize,
            );
            text.pos = text_pos;
            let size = (button_width as usize, block_height as usize);
            let block = rectangle::Rectangle::new(block_pos, size, None, Some([255, 100, 0, 0]));
            canvas.draw(&block);
            canvas.draw(&text);

            right_most_pixel = block_pos.0;
            (block_pos, size)
        };

        let (position, size) = draw_button("x".into(), &self.font_data, &mut canvas);
        let click_target = ClickTarget {
            position,
            size,
            handler: ClickHandler::Exit,
        };
        self.click_targets.push(click_target);

        for button in self.args.buttons.iter().cloned() {
            let (position, size) = draw_button(button.text, &self.font_data, &mut canvas);
            let click_target = ClickTarget {
                position,
                size,
                handler: ClickHandler::RunCommand(button.action),
            };
            self.click_targets.push(click_target);
        }

        // Draw message
        let text = text::Text::new(
            (horizontal_padding, height as usize / 2 - text_hh as usize),
            FONT_COLOR,
            &self.font_data,
            text_h,
            1.0,
            &self.args.message,
        );
        canvas.draw(&text);

        pool.seek(SeekFrom::Start(0)).unwrap();
        pool.write_all(canvas.buffer).unwrap();
        pool.flush().unwrap();

        // Create a new buffer from the pool
        let buffer = pool.buffer(0, width, height, stride, wl_shm::Format::Argb8888);

        // Attach the buffer to the surface and mark the entire surface as damaged
        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, width as i32, height as i32);

        // Finally, commit the surface
        surface.commit();
    }
}

impl ClickTarget {
    fn process_click(&self, click_position: (f64, f64)) -> Option<ClickHandler> {
        let (click_x, click_y) = click_position;
        let (position_x, position_y) = (self.position.0 as f64, self.position.1 as f64);
        let (size_x, size_y) = (self.size.0 as f64, self.size.1 as f64);

        if click_x >= position_x
            && click_x < position_x + size_x
            && click_y >= position_y
            && click_y < position_y + size_y
        {
            Some(self.handler.clone())
        } else {
            None
        }
    }
}

fn main() {
    let mut args = match args::parse(env::args()) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{}", message);

            process::exit(1);
        }
    };

    if args.detailed_message {
        let result = io::stdin().read_to_string(&mut args.detailed_message_contents);

        // Don't fail if we can't read this into a string, just print a message
        // for debugging purposes.
        if let Err(e) = result {
            eprintln!("WARN: failed to read detailed message from stdin {}", e);
        }
    }

    // TODO
    // handle type warn vs error

    // TODO make this a match statement
    if args.window_type == WindowType::Bar {
        let (env, display, queue) =
            init_default_environment!(Env, fields = [layer_shell: SimpleGlobal::new(),])
                .expect("Initial roundtrip failed!");

        let surfaces = Rc::new(RefCell::new(Vec::new()));

        let layer_shell = env.require_global::<zwlr_layer_shell_v1::ZwlrLayerShellV1>();

        let env_handle = env.clone();
        let surfaces_handle = Rc::clone(&surfaces);
        let output_handler = move |output: wl_output::WlOutput, info: &OutputInfo| {
            if info.obsolete {
                // an output has been removed, release it
                surfaces_handle
                    .borrow_mut()
                    .retain(|(i, _, _)| *i != info.id);
                output.release();
            } else {
                // an output has been created, construct a surface for it
                let surface = env_handle.create_surface().detach();
                let pools = env_handle
                    .create_double_pool(|_| {})
                    .expect("Failed to create a memory pool!");
                let layer_surface = layer_shell.get_layer_surface(
                    &surface,
                    Some(&output),
                    zwlr_layer_shell_v1::Layer::Overlay,
                    "example".to_owned(),
                );

                let height = 32;
                layer_surface.set_size(0, height);
                layer_surface.set_anchor(
                    zwlr_layer_surface_v1::Anchor::Top
                        | zwlr_layer_surface_v1::Anchor::Left
                        | zwlr_layer_surface_v1::Anchor::Right,
                );
                layer_surface.set_exclusive_zone(height as i32);

                let next_render_event = Rc::new(Cell::new(None::<RenderEvent>));
                let next_render_event_handle = Rc::clone(&next_render_event);
                layer_surface.quick_assign(move |layer_surface, event, _| {
                    match (event, next_render_event_handle.get()) {
                        (zwlr_layer_surface_v1::Event::Closed, _) => {
                            next_render_event_handle.set(Some(RenderEvent::Closed));
                        }
                        (
                            zwlr_layer_surface_v1::Event::Configure {
                                serial,
                                width,
                                height,
                            },
                            next,
                        ) if next != Some(RenderEvent::Closed) => {
                            layer_surface.ack_configure(serial);
                            next_render_event_handle
                                .set(Some(RenderEvent::Configure { width, height }));
                        }
                        (_, _) => {}
                    }
                });
                surface.commit();

                (*surfaces_handle.borrow_mut()).push((
                    info.id,
                    Surface::new(args.clone(), pools, next_render_event),
                    surface,
                ));
            }
        };

        for seat in env.get_all_seats() {
            if let Some(has_ptr) = seat::with_seat_data(&seat, |seat_data| {
                seat_data.has_pointer && !seat_data.defunct
            }) {
                if has_ptr {
                    let pointer = seat.get_pointer();
                    // let surface = window.surface().clone();
                    let surfaces_handle = surfaces.clone();
                    pointer.quick_assign(move |_, event, _| {
                        for surface in (*surfaces_handle).borrow_mut().iter_mut() {
                            // We should be filtering this down so we only pass
                            // the event on to the appropriate surface. TODO
                            surface.1.handle_pointer_event(&event);
                        }
                    });
                }
            }
        }

        // Process currently existing outputs
        for output in env.get_all_outputs() {
            if let Some(info) = with_output_info(&output, Clone::clone) {
                output_handler(output, &info);
            }
        }

        // Setup a listener for changes
        // The listener will live for as long as we keep this handle alive
        let _listner_handle =
            env.listen_for_outputs(move |output, info, _| output_handler(output, info));

        let mut event_loop = calloop::EventLoop::<()>::new().unwrap();

        WaylandSource::new(queue)
            .quick_insert(event_loop.handle())
            .unwrap();

        loop {
            // This is ugly, let's hope that some version of drain_filter() gets stabilized soon
            // https://github.com/rust-lang/rust/issues/43244
            {
                let mut surfaces = surfaces.borrow_mut();
                let mut i = 0;
                while i != surfaces.len() {
                    let (_s0, s1, s2) = &mut surfaces[i];
                    if s1.handle_events(s2) {
                        surfaces.remove(i);
                    } else {
                        i += 1;
                    }
                }
            }

            // Return early here if all surface are gone, otherwise the event loop
            // dispatch will panic with an error about not handling an event.
            if surfaces.borrow().is_empty() {
                return;
            }

            display.flush().unwrap();
            event_loop.dispatch(None, &mut ()).unwrap();
        }
    } else {
        let (env, display, queue) =
            init_default_environment!(NormalWindowEnv, desktop).expect("Initial roundtrip failed!");
        /*
         * Prepare a calloop event loop to handle key repetion
         */
        // Here `Option<WEvent>` is the type of a global value that will be shared by
        // all callbacks invoked by the event loop.
        let mut event_loop = calloop::EventLoop::<Option<window::Event>>::new().unwrap();

        /*
         * Create a buffer with window contents
         */

        let dimensions = (320u32, 240u32);

        /*
         * Init wayland objects
         */

        let surface = env.create_surface().detach();

        let mut window = env
            .create_window::<ConceptFrame, _>(surface, dimensions, move |evt, mut dispatch_data| {
                let next_action = dispatch_data.get::<Option<window::Event>>().unwrap();
                // Keep last event in priority order : Close > Configure > Refresh
                let replace = match (&evt, &*next_action) {
                    (_, &None)
                    | (_, &Some(window::Event::Refresh))
                    | (&window::Event::Configure { .. }, &Some(window::Event::Configure { .. }))
                    | (&window::Event::Close, _) => true,
                    _ => false,
                };
                if replace {
                    *next_action = Some(evt);
                }
            })
            .expect("Failed to create a window !");

        window.set_title("Kbd Input".to_string());
        window.set_app_id("waysay".into());

        let pools = env
            .create_double_pool(|_| {})
            .expect("Failed to create a memory pool !");

        /*
         * Keyboard initialization
         */

        let mut seats = Vec::<(
            String,
            Option<(wl_keyboard::WlKeyboard, calloop::Source<_>)>,
        )>::new();

        // first process already existing seats
        for seat in env.get_all_seats() {
            if let Some((has_kbd, name)) = seat::with_seat_data(&seat, |seat_data| {
                (
                    seat_data.has_keyboard && !seat_data.defunct,
                    seat_data.name.clone(),
                )
            }) {
                if has_kbd {
                    let _seat_name = name.clone();
                    match map_keyboard_repeat(
                        event_loop.handle(),
                        &seat,
                        None,
                        RepeatKind::System,
                        // TODO handle key here
                        move |_event, _, _| {},
                    ) {
                        Ok((kbd, repeat_source)) => {
                            seats.push((name, Some((kbd, repeat_source))));
                        }
                        Err(e) => {
                            eprintln!("Failed to map keyboard on seat {} : {:?}.", name, e);
                            seats.push((name, None));
                        }
                    }
                } else {
                    seats.push((name, None));
                }
            }
        }

        // then setup a listener for changes
        let loop_handle = event_loop.handle();
        let _seat_listener = env.listen_for_seats(move |seat, seat_data, _| {
            // find the seat in the vec of seats, or insert it if it is unknown
            let idx = seats.iter().position(|(name, _)| name == &seat_data.name);
            let idx = idx.unwrap_or_else(|| {
                seats.push((seat_data.name.clone(), None));
                seats.len() - 1
            });

            let (_, ref mut opt_kbd) = &mut seats[idx];
            // we should map a keyboard if the seat has the capability & is not defunct
            if seat_data.has_keyboard && !seat_data.defunct {
                if opt_kbd.is_none() {
                    // we should initalize a keyboard
                    let _seat_name = seat_data.name.clone();
                    match map_keyboard_repeat(
                        loop_handle.clone(),
                        &seat,
                        None,
                        RepeatKind::System,
                        move |_event, _, _| {}, // TODO handle key
                    ) {
                        Ok((kbd, repeat_source)) => {
                            *opt_kbd = Some((kbd, repeat_source));
                        }
                        Err(e) => eprintln!(
                            "Failed to map keyboard on seat {} : {:?}.",
                            seat_data.name, e
                        ),
                    }
                }
            } else if let Some((kbd, source)) = opt_kbd.take() {
                // the keyboard has been removed, cleanup
                kbd.release();
                loop_handle.remove(source);
            }
        });

        let next_render_event = Rc::new(Cell::new(None::<RenderEvent>));
        let _next_render_event_handle = Rc::clone(&next_render_event);
        let mut my_surface = Surface::new(args, pools, next_render_event);
        // TODO does this make sense?
        // configure default dimensions to those chosen when creating the window
        my_surface.dimensions = dimensions;

        let mut next_action = None;

        WaylandSource::new(queue)
            .quick_insert(event_loop.handle())
            .unwrap();

        loop {
            match next_action.take() {
                Some(window::Event::Close) => break,
                Some(window::Event::Refresh) => {
                    window.refresh();
                    window.surface().commit();
                }
                Some(window::Event::Configure {
                    new_size,
                    states: _,
                }) => {
                    if let Some((w, h)) = new_size {
                        window.resize(w, h);
                        my_surface.dimensions = (w, h);
                    }
                    my_surface.draw(window.surface());
                    window.refresh();
                }
                None => {}
            }

            // always flush the connection before going to sleep waiting for events
            display.flush().unwrap();

            event_loop.dispatch(None, &mut next_action).unwrap();
        }
    };
}
