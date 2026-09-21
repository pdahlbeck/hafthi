mod imp {
    use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
    use wayland_client::backend::{Backend, ObjectId};
    use wayland_client::protocol::{
        wl_registry::{self, WlRegistry},
        wl_surface::WlSurface,
    };
    use wayland_client::{
        delegate_noop, Connection, Dispatch, EventQueue, Proxy, QueueHandle,
    };
    use winit::window::Window;

    mod protocol {
        use wayland_client;
        use wayland_client::protocol::*;

        pub mod __interfaces {
            use wayland_client::protocol::__interfaces::*;
            wayland_scanner::generate_interfaces!("src/protos/ext-background-effect-v1.xml");
        }

        use self::__interfaces::*;
        wayland_scanner::generate_client_code!("src/protos/ext-background-effect-v1.xml");
    }

    use protocol::ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1;
    use protocol::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;

    #[derive(Default)]
    struct State {
        manager: Option<ExtBackgroundEffectManagerV1>,
    }

    impl Dispatch<WlRegistry, ()> for State {
        fn event(
            state: &mut Self,
            registry: &WlRegistry,
            event: wl_registry::Event,
            _: &(),
            _: &Connection,
            qh: &QueueHandle<Self>,
        ) {
            let wl_registry::Event::Global {
                name,
                interface,
                version,
            } = event
            else {
                return;
            };

            if interface == "ext_background_effect_manager_v1" && state.manager.is_none() {
                state.manager = Some(registry.bind(name, version.min(1), qh, ()));
            }
        }
    }

    impl Dispatch<ExtBackgroundEffectManagerV1, ()> for State {
        fn event(
            _: &mut Self,
            _: &ExtBackgroundEffectManagerV1,
            _: <ExtBackgroundEffectManagerV1 as Proxy>::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    delegate_noop!(State: ignore ExtBackgroundEffectSurfaceV1);

    pub struct NoBlur {
        conn: Connection,
        queue: EventQueue<State>,
        state: State,
        _surface: WlSurface,
        _effect: ExtBackgroundEffectSurfaceV1,
    }

    impl NoBlur {
        pub fn attach(window: &Window) -> Option<Self> {
            let display = window.display_handle().ok()?.as_raw();
            let surface = window.window_handle().ok()?.as_raw();

            let RawDisplayHandle::Wayland(display) = display else {
                return None;
            };
            let RawWindowHandle::Wayland(surface_handle) = surface else {
                return None;
            };

            // SAFETY: both pointers are borrowed from the live winit Window.
            // This backend does not own or destroy winit's wl_display.
            let backend =
                unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) };
            let conn = Connection::from_backend(backend);

            let mut queue: EventQueue<State> = conn.new_event_queue();
            let qh = queue.handle();
            let mut state = State::default();
            let _registry = conn.display().get_registry(&qh, ());

            queue.roundtrip(&mut state).ok()?;
            let manager = state.manager.take()?;

            // SAFETY: this is winit's live wl_surface on the same connection.
            let surface_id = unsafe {
                ObjectId::from_ptr(
                    WlSurface::interface(),
                    surface_handle.surface.as_ptr().cast(),
                )
                .ok()?
            };
            let wl_surface = WlSurface::from_id(&conn, surface_id).ok()?;

            let effect = manager.get_background_effect(&wl_surface, &qh, ());

            // ext-background-effect-v1 defines an empty blur region as no blur.
            // Hyprland gives this protocol state precedence over its default
            // blur policy for translucent windows.
            effect.set_blur_region(None);
            manager.destroy();

            let _ = conn.flush();

            eprintln!("Hafthi: ext-background-effect-v1 no-blur attached");

            Some(Self {
                conn,
                queue,
                state,
                _surface: wl_surface,
                _effect: effect,
            })
        }

        pub fn dispatch_pending(&mut self) {
            let _ = self.queue.dispatch_pending(&mut self.state);
            let _ = self.conn.flush();
        }
    }
}

pub use imp::NoBlur;
