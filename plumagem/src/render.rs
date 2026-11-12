use smithay::{
    backend::renderer::{
        damage::{Error as OutputDamageTrackerError, OutputDamageTracker, RenderOutputResult},
        element::{
            surface::WaylandSurfaceRenderElement,
            utils::{
                ConstrainAlign, ConstrainScaleBehavior, CropRenderElement, RelocateRenderElement,
                RescaleRenderElement,
            },
            memory::MemoryRenderBufferRenderElement,
            solid::SolidColorRenderElement,
            AsRenderElements, RenderElement, Wrap,
        },
        Color32F, ImportAll, ImportMem, Renderer,
    },
    desktop::space::{
        constrain_space_element, ConstrainBehavior, ConstrainReference, Space, SpaceRenderElements,
    },
    output::Output,
    utils::{Point, Rectangle, Size},
};

#[cfg(feature = "debug")]
use crate::drawing::FpsElement;
use crate::{
    drawing::{PointerRenderElement, CLEAR_COLOR, CLEAR_COLOR_FULLSCREEN},
    shell::{FullscreenSurface, WindowElement, WindowRenderElement},
};

smithay::backend::renderer::element::render_elements! {
    pub CustomRenderElements<R> where
        R: ImportAll + ImportMem;
    Pointer=PointerRenderElement<R>,
    Surface=WaylandSurfaceRenderElement<R>,
    #[cfg(feature = "debug")]
    // Note: We would like to borrow this element instead, but that would introduce
    // a feature-dependent lifetime, which introduces a lot more feature bounds
    // as the whole type changes and we can't have an unused lifetime (for when "debug" is disabled)
    // in the declaration.
    Fps=FpsElement<R::TextureId>,
}

impl<R: Renderer> std::fmt::Debug for CustomRenderElements<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pointer(arg0) => f.debug_tuple("Pointer").field(arg0).finish(),
            Self::Surface(arg0) => f.debug_tuple("Surface").field(arg0).finish(),
            #[cfg(feature = "debug")]
            Self::Fps(arg0) => f.debug_tuple("Fps").field(arg0).finish(),
            Self::_GenericCatcher(arg0) => f.debug_tuple("_GenericCatcher").field(arg0).finish(),
        }
    }
}

smithay::backend::renderer::element::render_elements! {
    pub OutputRenderElements<R, E> where R: ImportAll + ImportMem;
    Space=SpaceRenderElements<R, E>,
    Window=Wrap<E>,
    Custom=CustomRenderElements<R>,
    Preview=CropRenderElement<RelocateRenderElement<RescaleRenderElement<WindowRenderElement<R>>>>,
    Fundo=MemoryRenderBufferRenderElement<R>,
    Solido=SolidColorRenderElement,
}

impl<R: Renderer + ImportAll + ImportMem, E: RenderElement<R> + std::fmt::Debug> std::fmt::Debug
    for OutputRenderElements<R, E>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Space(arg0) => f.debug_tuple("Space").field(arg0).finish(),
            Self::Window(arg0) => f.debug_tuple("Window").field(arg0).finish(),
            Self::Custom(arg0) => f.debug_tuple("Custom").field(arg0).finish(),
            Self::Preview(arg0) => f.debug_tuple("Preview").field(arg0).finish(),
            Self::Fundo(arg0) => f.debug_tuple("Fundo").field(arg0).finish(),
            Self::Solido(arg0) => f.debug_tuple("Solido").field(arg0).finish(),
            Self::_GenericCatcher(arg0) => f.debug_tuple("_GenericCatcher").field(arg0).finish(),
        }
    }
}

pub fn space_preview_elements<'a, R, C>(
    renderer: &'a mut R,
    space: &'a Space<WindowElement>,
    output: &'a Output,
) -> impl Iterator<Item = C> + 'a
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
    C: From<CropRenderElement<RelocateRenderElement<RescaleRenderElement<WindowRenderElement<R>>>>> + 'a,
{
    let constrain_behavior = ConstrainBehavior {
        reference: ConstrainReference::BoundingBox,
        behavior: ConstrainScaleBehavior::Fit,
        align: ConstrainAlign::CENTER,
    };

    let preview_padding = 10;

    let elements_on_space = space.elements_for_output(output).count();
    let output_scale = output.current_scale().fractional_scale();
    let output_transform = output.current_transform();
    let output_size = output
        .current_mode()
        .map(|mode| {
            output_transform
                .transform_size(mode.size)
                .to_f64()
                .to_logical(output_scale)
        })
        .unwrap_or_default();

    let max_elements_per_row = 4;
    let elements_per_row = usize::min(elements_on_space, max_elements_per_row);
    let rows = f64::ceil(elements_on_space as f64 / elements_per_row as f64);

    let preview_size = Size::from((
        f64::round(output_size.w / elements_per_row as f64) as i32 - preview_padding * 2,
        f64::round(output_size.h / rows) as i32 - preview_padding * 2,
    ));

    space
        .elements_for_output(output)
        .enumerate()
        .flat_map(move |(element_index, window)| {
            let column = element_index % elements_per_row;
            let row = element_index / elements_per_row;
            let preview_location = Point::from((
                preview_padding + (preview_padding + preview_size.w) * column as i32,
                preview_padding + (preview_padding + preview_size.h) * row as i32,
            ));
            let constrain = Rectangle::new(preview_location, preview_size);
            constrain_space_element(
                renderer,
                window,
                preview_location,
                1.0,
                output_scale,
                constrain,
                constrain_behavior,
            )
        })
}

/// Captura com recorte ativa: a camada escura vai por cima das janelas.
pub static RECORTE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[profiling::function]
pub fn output_elements<R>(
    output: &Output,
    space: &Space<WindowElement>,
    custom_elements: impl IntoIterator<Item = CustomRenderElements<R>>,
    renderer: &mut R,
    show_window_preview: bool,
    fundo: Option<&crate::fundo::Fundo>,
    interface: Vec<MemoryRenderBufferRenderElement<R>>,
    selecao: Vec<SolidColorRenderElement>,
    area: Vec<MemoryRenderBufferRenderElement<R>>,
) -> (Vec<OutputRenderElements<R, WindowRenderElement<R>>>, Color32F)
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    if let Some(window) = output
        .user_data()
        .get::<FullscreenSurface>()
        .and_then(|f| f.get())
    {
        let scale = output.current_scale().fractional_scale().into();
        let window_render_elements: Vec<WindowRenderElement<R>> =
            AsRenderElements::<R>::render_elements(&window, renderer, (0, 0).into(), scale, 1.0);

        let elements = custom_elements
            .into_iter()
            .map(OutputRenderElements::from)
            .chain(
                window_render_elements
                    .into_iter()
                    .map(|e| OutputRenderElements::Window(Wrap::from(e))),
            )
            .collect::<Vec<_>>();
        (elements, CLEAR_COLOR_FULLSCREEN)
    } else {
        let mut output_render_elements = custom_elements
            .into_iter()
            .map(OutputRenderElements::from)
            .collect::<Vec<_>>();
        // Miniaturas da prévia da barra: por cima da interface.
        let previas = crate::painel::PREVIAS.with(|p| p.borrow().clone());
        if !previas.is_empty() {
            let escala = output.current_scale().fractional_scale();
            let origem = space.output_geometry(output).map(|g| g.loc).unwrap_or_default();
            let comportamento = ConstrainBehavior {
                reference: ConstrainReference::BoundingBox,
                behavior: ConstrainScaleBehavior::Fit,
                align: ConstrainAlign::CENTER,
            };
            for (w, r) in previas {
                let r = Rectangle::new(r.loc - origem, r.size);
                output_render_elements.extend(
                    constrain_space_element(renderer, &w, r.loc, 1.0, escala, r, comportamento)
                        .map(OutputRenderElements::Preview),
                );
            }
        }
        let custom_n = output_render_elements.len();

        // Barra de tarefas e menu: acima das janelas, abaixo do cursor.
        output_render_elements.extend(interface.into_iter().map(OutputRenderElements::Fundo));

        if show_window_preview && space.elements_for_output(output).count() > 0 {
            output_render_elements.extend(space_preview_elements(renderer, space, output));
        }

        let space_elements = smithay::desktop::space::space_render_elements::<_, WindowElement, _>(
            renderer,
            [space],
            output,
            1.0,
        )
        .expect("output without mode?");
        output_render_elements.extend(space_elements.into_iter().map(OutputRenderElements::Space));

        // Seleção na área de trabalho: abaixo das janelas, acima do fundo.
        // No recorte de captura, por cima de tudo (menos o cursor).
        if RECORTE.load(std::sync::atomic::Ordering::Relaxed) {
            let n = custom_n;
            let topo: Vec<_> = selecao.into_iter().map(OutputRenderElements::Solido).collect();
            for (i, e) in topo.into_iter().enumerate() {
                output_render_elements.insert(n + i, e);
            }
        } else {
            output_render_elements.extend(selecao.into_iter().map(OutputRenderElements::Solido));
        }
        // Ícones da área de trabalho, logo acima do papel de parede.
        output_render_elements.extend(area.into_iter().map(OutputRenderElements::Fundo));

        // O papel de parede fica por último: é a camada mais funda.
        if let Some(elemento) = fundo.and_then(|f| f.elemento(renderer, output)) {
            output_render_elements.push(OutputRenderElements::Fundo(elemento));
        }

        // Renderização por software: uma camada sólida que muda (de forma
        // invisível) a cada quadro faz a tela inteira ser redesenhada.
        if REDESENHO_TOTAL.load(std::sync::atomic::Ordering::Relaxed)
            || std::env::var_os("PLUMAGEM_REDESENHO_TOTAL").is_some()
        {
            if let Some(geo) = space.output_geometry(output) {
                let escala = output.current_scale().fractional_scale();
                CAMADA_TOTAL.with(|c| {
                    let mut c = c.borrow_mut();
                    c.1 = !c.1;
                    // Quase transparente: só serve para marcar a tela inteira.
                    let a = if c.1 { 0.001 } else { 0.002 };
                    c.0.update(geo.size, [0.0, 0.0, 0.0, a]);
                    output_render_elements.push(OutputRenderElements::Solido(SolidColorRenderElement::from_buffer(
                        &c.0,
                        (0, 0),
                        escala,
                        1.0,
                        smithay::backend::renderer::element::Kind::Unspecified,
                    )));
                });
            }
        }

        (output_render_elements, CLEAR_COLOR)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render_output<'a, 'd, R>(
    output: &'a Output,
    space: &'a Space<WindowElement>,
    custom_elements: impl IntoIterator<Item = CustomRenderElements<R>>,
    renderer: &'a mut R,
    framebuffer: &'a mut R::Framebuffer<'_>,
    damage_tracker: &'d mut OutputDamageTracker,
    age: usize,
    show_window_preview: bool,
) -> Result<RenderOutputResult<'d>, OutputDamageTrackerError<R::Error>>
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    let (elements, clear_color) =
        output_elements(output, space, custom_elements, renderer, show_window_preview, None, Vec::new(), Vec::new(), Vec::new());
    damage_tracker.render_output(renderer, framebuffer, age, &elements, clear_color)
}

/// Redesenhar a tela inteira a cada quadro (ligado com renderização por
/// software: com o llvmpipe, desenhar só as áreas danificadas deixava rastros
/// do cursor). Também pode ser ligado com PLUMAGEM_REDESENHO_TOTAL=1.
pub static REDESENHO_TOTAL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

thread_local! {
    static CAMADA_TOTAL: std::cell::RefCell<(smithay::backend::renderer::element::solid::SolidColorBuffer, bool)> =
        std::cell::RefCell::new((Default::default(), false));
}
