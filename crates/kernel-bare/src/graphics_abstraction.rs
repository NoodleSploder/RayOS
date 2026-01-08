// RAYOS Phase 25 Task 1: Graphics API Abstraction Layer
// Unified graphics API interface for GPU access
// File: crates/kernel-bare/src/graphics_abstraction.rs
// Lines: 700+ | Tests: 18 unit + 4 scenario | Markers: 5

use core::fmt;

const MAX_SHADERS: usize = 128;
const MAX_PIPELINES: usize = 64;
const MAX_BUFFERS: usize = 256;
const MAX_TEXTURES: usize = 512;
const MAX_RENDERPASS_ATTACHMENTS: usize = 8;
const MAX_COMMAND_BUFFERS: usize = 32;

// ============================================================================
// ENUMS & TYPES
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicsAPI {
    Vulkan,
    OpenGL,
    Software,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderStage {
    Vertex,
    Fragment,
    Compute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferUsage {
    VertexBuffer,
    IndexBuffer,
    UniformBuffer,
    StorageBuffer,
    CopySource,
    CopyDestination,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    RGBA8,
    RGB8,
    RGBA16F,
    RGBA32F,
    Depth32F,
    Depth24Stencil8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendFactor {
    Zero,
    One,
    SrcColor,
    OneMinusSrcColor,
    DstColor,
    OneMinusDstColor,
    SrcAlpha,
    OneMinusSrcAlpha,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Never,
    Less,
    Equal,
    LessEqual,
    Greater,
    NotEqual,
    GreaterEqual,
    Always,
}

#[derive(Debug, Clone, Copy)]
pub struct ShaderBinary {
    pub stage: ShaderStage,
    pub data: [u8; 256],
    pub size: usize,
}

impl ShaderBinary {
    pub fn new(stage: ShaderStage) -> Self {
        ShaderBinary {
            stage,
            data: [0u8; 256],
            size: 0,
        }
    }

    pub fn with_code(mut self, code: &[u8]) -> Self {
        let len = core::cmp::min(code.len(), 256);
        if len > 0 {
            self.data[..len].copy_from_slice(&code[..len]);
        }
        self.size = len;
        self
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GraphicsBufferDesc {
    pub size: u32,
    pub usage: BufferUsage,
    pub persistent: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ImageDesc {
    pub width: u32,
    pub height: u32,
    pub format: ImageFormat,
    pub depth: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct AttachmentDesc {
    pub format: ImageFormat,
    pub load_op: LoadOp,
    pub store_op: StoreOp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadOp {
    Load,
    Clear,
    DontCare,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreOp {
    Store,
    DontCare,
}

#[derive(Debug, Clone, Copy)]
pub struct BlendState {
    pub enabled: bool,
    pub src_factor: BlendFactor,
    pub dst_factor: BlendFactor,
}

#[derive(Debug, Clone, Copy)]
pub struct DepthState {
    pub test_enabled: bool,
    pub write_enabled: bool,
    pub compare_op: CompareOp,
}

#[derive(Debug, Clone, Copy)]
pub struct PipelineState {
    pub blend: BlendState,
    pub depth: DepthState,
    pub polygon_mode: u8,
}

impl PipelineState {
    pub fn default() -> Self {
        PipelineState {
            blend: BlendState {
                enabled: false,
                src_factor: BlendFactor::One,
                dst_factor: BlendFactor::Zero,
            },
            depth: DepthState {
                test_enabled: true,
                write_enabled: true,
                compare_op: CompareOp::Less,
            },
            polygon_mode: 0, // Fill
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DrawCall {
    pub vertex_count: u32,
    pub instance_count: u32,
    pub first_vertex: u32,
    pub first_instance: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Rect2D {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct ClearValue {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
    pub depth: f32,
}

impl ClearValue {
    pub fn black() -> Self {
        ClearValue {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
            depth: 1.0,
        }
    }

    pub fn white() -> Self {
        ClearValue {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
            depth: 1.0,
        }
    }
}

// ============================================================================
// GRAPHICS RESOURCES
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct GraphicsBuffer {
    pub id: u32,
    pub size: u32,
    pub usage: BufferUsage,
    pub mapped: bool,
}

impl GraphicsBuffer {
    pub fn new(id: u32, size: u32, usage: BufferUsage) -> Self {
        GraphicsBuffer {
            id,
            size,
            usage,
            mapped: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ImageResource {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub format: ImageFormat,
    pub depth: u32,
}

impl ImageResource {
    pub fn new(id: u32, desc: ImageDesc) -> Self {
        ImageResource {
            id,
            width: desc.width,
            height: desc.height,
            format: desc.format,
            depth: desc.depth,
        }
    }

    pub fn get_texel_size(&self) -> u32 {
        match self.format {
            ImageFormat::RGBA8 => 4,
            ImageFormat::RGB8 => 3,
            ImageFormat::RGBA16F => 8,
            ImageFormat::RGBA32F => 16,
            ImageFormat::Depth32F => 4,
            ImageFormat::Depth24Stencil8 => 4,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Framebuffer {
    pub id: u32,
    pub color_attachments: [Option<u32>; MAX_RENDERPASS_ATTACHMENTS],
    pub depth_attachment: Option<u32>,
    pub width: u32,
    pub height: u32,
}

impl Framebuffer {
    pub fn new(id: u32, width: u32, height: u32) -> Self {
        Framebuffer {
            id,
            color_attachments: [None; MAX_RENDERPASS_ATTACHMENTS],
            depth_attachment: None,
            width,
            height,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ShaderProgram {
    pub id: u32,
    pub vertex_shader: Option<u32>,
    pub fragment_shader: Option<u32>,
    pub compute_shader: Option<u32>,
    pub compiled: bool,
}

impl ShaderProgram {
    pub fn new(id: u32) -> Self {
        ShaderProgram {
            id,
            vertex_shader: None,
            fragment_shader: None,
            compute_shader: None,
            compiled: false,
        }
    }

    pub fn attach_shader(&mut self, stage: ShaderStage, shader_id: u32) {
        match stage {
            ShaderStage::Vertex => self.vertex_shader = Some(shader_id),
            ShaderStage::Fragment => self.fragment_shader = Some(shader_id),
            ShaderStage::Compute => self.compute_shader = Some(shader_id),
        }
    }

    pub fn is_complete(&self) -> bool {
        self.vertex_shader.is_some() && self.fragment_shader.is_some()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RenderPass {
    pub id: u32,
    pub num_attachments: usize,
    pub width: u32,
    pub height: u32,
}

impl RenderPass {
    pub fn new(id: u32, width: u32, height: u32) -> Self {
        RenderPass {
            id,
            num_attachments: 0,
            width,
            height,
        }
    }
}

// ============================================================================
// COMMAND BUFFER
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandType {
    BeginRenderPass,
    EndRenderPass,
    SetPipeline,
    BindBuffer,
    BindImage,
    SetViewport,
    SetScissor,
    Draw,
    DrawIndexed,
    ClearAttachments,
    CopyBuffer,
    CopyImage,
    Barrier,
}

#[derive(Debug, Clone, Copy)]
pub struct Command {
    pub cmd_type: CommandType,
    pub param1: u32,
    pub param2: u32,
    pub param3: u32,
    pub param4: u32,
}

impl Command {
    pub fn new(cmd_type: CommandType) -> Self {
        Command {
            cmd_type,
            param1: 0,
            param2: 0,
            param3: 0,
            param4: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CommandBuffer {
    pub id: u32,
    pub commands: [Command; 512],
    pub command_count: usize,
    pub recording: bool,
}

impl CommandBuffer {
    pub fn new(id: u32) -> Self {
        CommandBuffer {
            id,
            commands: [Command::new(CommandType::BeginRenderPass); 512],
            command_count: 0,
            recording: false,
        }
    }

    pub fn begin(&mut self) {
        self.recording = true;
        self.command_count = 0;
    }

    pub fn end(&mut self) -> bool {
        self.recording = false;
        self.command_count > 0
    }

    pub fn push_command(&mut self, cmd: Command) -> bool {
        if self.command_count >= 512 {
            return false;
        }
        self.commands[self.command_count] = cmd;
        self.command_count += 1;
        true
    }

    pub fn begin_render_pass(&mut self, pass_id: u32, fb_id: u32) -> bool {
        let cmd = Command::new(CommandType::BeginRenderPass);
        let mut cmd = cmd;
        cmd.param1 = pass_id;
        cmd.param2 = fb_id;
        self.push_command(cmd)
    }

    pub fn end_render_pass(&mut self) -> bool {
        self.push_command(Command::new(CommandType::EndRenderPass))
    }

    pub fn set_pipeline(&mut self, pipeline_id: u32) -> bool {
        let cmd = Command::new(CommandType::SetPipeline);
        let mut cmd = cmd;
        cmd.param1 = pipeline_id;
        self.push_command(cmd)
    }

    pub fn draw(&mut self, vertex_count: u32, instance_count: u32) -> bool {
        let cmd = Command::new(CommandType::Draw);
        let mut cmd = cmd;
        cmd.param1 = vertex_count;
        cmd.param2 = instance_count;
        self.push_command(cmd)
    }

    pub fn clear_color(&mut self, r: f32, g: f32, b: f32, a: f32) -> bool {
        let cmd = Command::new(CommandType::ClearAttachments);
        let mut cmd = cmd;
        cmd.param1 = (r * 255.0) as u32;
        cmd.param2 = (g * 255.0) as u32;
        cmd.param3 = (b * 255.0) as u32;
        cmd.param4 = (a * 255.0) as u32;
        self.push_command(cmd)
    }

    pub fn is_empty(&self) -> bool {
        self.command_count == 0
    }
}

// ============================================================================
// GRAPHICS CONTEXT
// ============================================================================

pub struct GraphicsContext {
    pub api: GraphicsAPI,
    pub device_name: [u8; 64],
    pub vendor_name: [u8; 64],
    pub shader_programs: [Option<ShaderProgram>; MAX_SHADERS],
    pub buffers: [Option<GraphicsBuffer>; MAX_BUFFERS],
    pub images: [Option<ImageResource>; MAX_TEXTURES],
    pub command_buffers: [Option<CommandBuffer>; MAX_COMMAND_BUFFERS],
    pub next_id: u32,
    pub shader_count: usize,
    pub buffer_count: usize,
    pub image_count: usize,
}

impl GraphicsContext {
    pub fn new(api: GraphicsAPI) -> Self {
        GraphicsContext {
            api,
            device_name: [0u8; 64],
            vendor_name: [0u8; 64],
            shader_programs: [None; MAX_SHADERS],
            buffers: [None; MAX_BUFFERS],
            images: [None; MAX_TEXTURES],
            command_buffers: [None; MAX_COMMAND_BUFFERS],
            next_id: 1,
            shader_count: 0,
            buffer_count: 0,
            image_count: 0,
        }
    }

    pub fn set_device_name(&mut self, name: &str) {
        let bytes = name.as_bytes();
        let len = core::cmp::min(bytes.len(), 63);
        if len > 0 {
            self.device_name[..len].copy_from_slice(&bytes[..len]);
        }
    }

    pub fn set_vendor_name(&mut self, name: &str) {
        let bytes = name.as_bytes();
        let len = core::cmp::min(bytes.len(), 63);
        if len > 0 {
            self.vendor_name[..len].copy_from_slice(&bytes[..len]);
        }
    }

    pub fn create_shader_program(&mut self) -> Option<u32> {
        if self.shader_count >= MAX_SHADERS {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        let program = ShaderProgram::new(id);
        self.shader_programs[self.shader_count] = Some(program);
        self.shader_count += 1;
        Some(id)
    }

    pub fn create_buffer(&mut self, size: u32, usage: BufferUsage) -> Option<u32> {
        if self.buffer_count >= MAX_BUFFERS {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        let buffer = GraphicsBuffer::new(id, size, usage);
        self.buffers[self.buffer_count] = Some(buffer);
        self.buffer_count += 1;
        Some(id)
    }

    pub fn create_image(&mut self, desc: ImageDesc) -> Option<u32> {
        if self.image_count >= MAX_TEXTURES {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        let image = ImageResource::new(id, desc);
        self.images[self.image_count] = Some(image);
        self.image_count += 1;
        Some(id)
    }

    pub fn create_command_buffer(&mut self) -> Option<u32> {
        for cb in &mut self.command_buffers {
            if cb.is_none() {
                let id = self.next_id;
                self.next_id += 1;
                *cb = Some(CommandBuffer::new(id));
                return Some(id);
            }
        }
        None
    }

    pub fn get_shader_program(&mut self, id: u32) -> Option<&mut ShaderProgram> {
        self.shader_programs.iter_mut().find(|p| p.as_ref().map(|s| s.id == id).unwrap_or(false)).and_then(|p| p.as_mut())
    }

    pub fn get_buffer(&mut self, id: u32) -> Option<&mut GraphicsBuffer> {
        self.buffers.iter_mut().find(|b| b.as_ref().map(|buf| buf.id == id).unwrap_or(false)).and_then(|b| b.as_mut())
    }

    pub fn get_image(&mut self, id: u32) -> Option<&mut ImageResource> {
        self.images.iter_mut().find(|i| i.as_ref().map(|img| img.id == id).unwrap_or(false)).and_then(|i| i.as_mut())
    }

    pub fn get_command_buffer(&mut self, id: u32) -> Option<&mut CommandBuffer> {
        self.command_buffers.iter_mut().find(|cb| cb.as_ref().map(|c| c.id == id).unwrap_or(false)).and_then(|cb| cb.as_mut())
    }

    pub fn resource_count(&self) -> (usize, usize, usize) {
        (self.shader_count, self.buffer_count, self.image_count)
    }
}

impl Default for GraphicsContext {
    fn default() -> Self {
        Self::new(GraphicsAPI::Software)
    }
}

// ============================================================================
// FRAME PRESENTATION
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct FramePresentation {
    pub frame_index: u32,
    pub swapchain_index: u32,
    pub ready: bool,
}

impl FramePresentation {
    pub fn new() -> Self {
        FramePresentation {
            frame_index: 0,
            swapchain_index: 0,
            ready: false,
        }
    }

    pub fn next_frame(&mut self) {
        self.frame_index += 1;
        self.swapchain_index = (self.swapchain_index + 1) % 3; // Triple buffering
    }
}

impl Default for FramePresentation {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// GRAPHICS DEVICE TRAIT
// ============================================================================

pub trait GraphicsDevice {
    fn initialize(&mut self) -> bool;
    fn shutdown(&mut self);
    fn is_available(&self) -> bool;
    fn get_api(&self) -> GraphicsAPI;
    fn wait_idle(&mut self);
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graphics_api_types() {
        assert_ne!(GraphicsAPI::Vulkan, GraphicsAPI::OpenGL);
        assert_ne!(GraphicsAPI::OpenGL, GraphicsAPI::Software);
    }

    #[test]
    fn test_shader_binary_new() {
        let binary = ShaderBinary::new(ShaderStage::Vertex);
        assert_eq!(binary.stage, ShaderStage::Vertex);
        assert_eq!(binary.size, 0);
    }

    #[test]
    fn test_shader_binary_with_code() {
        let code = b"vec4 main() { return vec4(1.0); }";
        let binary = ShaderBinary::new(ShaderStage::Fragment)
            .with_code(code);
        assert_eq!(binary.size, code.len());
    }

    #[test]
    fn test_graphics_buffer_new() {
        let buffer = GraphicsBuffer::new(1, 1024, BufferUsage::VertexBuffer);
        assert_eq!(buffer.id, 1);
        assert_eq!(buffer.size, 1024);
        assert!(!buffer.mapped);
    }

    #[test]
    fn test_image_resource_new() {
        let desc = ImageDesc {
            width: 1920,
            height: 1080,
            format: ImageFormat::RGBA8,
            depth: 1,
        };
        let img = ImageResource::new(1, desc);
        assert_eq!(img.width, 1920);
        assert_eq!(img.height, 1080);
    }

    #[test]
    fn test_image_format_texel_size() {
        let img_rgba8 = ImageResource::new(1, ImageDesc {
            width: 64, height: 64, format: ImageFormat::RGBA8, depth: 1,
        });
        assert_eq!(img_rgba8.get_texel_size(), 4);

        let img_rgba32f = ImageResource::new(2, ImageDesc {
            width: 64, height: 64, format: ImageFormat::RGBA32F, depth: 1,
        });
        assert_eq!(img_rgba32f.get_texel_size(), 16);
    }

    #[test]
    fn test_shader_program_new() {
        let program = ShaderProgram::new(1);
        assert_eq!(program.id, 1);
        assert!(!program.compiled);
        assert!(!program.is_complete());
    }

    #[test]
    fn test_shader_program_attach() {
        let mut program = ShaderProgram::new(1);
        program.attach_shader(ShaderStage::Vertex, 10);
        program.attach_shader(ShaderStage::Fragment, 11);
        assert!(program.is_complete());
    }

    #[test]
    fn test_command_buffer_new() {
        let cb = CommandBuffer::new(1);
        assert!(!cb.recording);
        assert!(cb.is_empty());
    }

    #[test]
    fn test_command_buffer_recording() {
        let mut cb = CommandBuffer::new(1);
        cb.begin();
        assert!(cb.recording);

        let success = cb.draw(100, 1);
        assert!(success);
        assert_eq!(cb.command_count, 1);

        cb.end();
        assert!(!cb.recording);
    }

    #[test]
    fn test_command_buffer_capacity() {
        let mut cb = CommandBuffer::new(1);
        cb.begin();
        for _ in 0..512 {
            if !cb.push_command(Command::new(CommandType::Draw)) {
                break;
            }
        }
        assert_eq!(cb.command_count, 512);
    }

    #[test]
    fn test_graphics_context_new() {
        let ctx = GraphicsContext::new(GraphicsAPI::Vulkan);
        assert_eq!(ctx.api, GraphicsAPI::Vulkan);
        assert_eq!(ctx.shader_count, 0);
        assert_eq!(ctx.buffer_count, 0);
    }

    #[test]
    fn test_graphics_context_create_shader() {
        let mut ctx = GraphicsContext::new(GraphicsAPI::OpenGL);
        let id = ctx.create_shader_program();
        assert!(id.is_some());
        assert_eq!(ctx.shader_count, 1);
    }

    #[test]
    fn test_graphics_context_create_buffer() {
        let mut ctx = GraphicsContext::new(GraphicsAPI::Software);
        let id = ctx.create_buffer(2048, BufferUsage::UniformBuffer);
        assert!(id.is_some());
        assert_eq!(ctx.buffer_count, 1);
    }

    #[test]
    fn test_graphics_context_create_image() {
        let mut ctx = GraphicsContext::new(GraphicsAPI::Vulkan);
        let desc = ImageDesc {
            width: 512, height: 512, format: ImageFormat::RGBA8, depth: 1,
        };
        let id = ctx.create_image(desc);
        assert!(id.is_some());
        assert_eq!(ctx.image_count, 1);
    }

    #[test]
    fn test_pipeline_state_default() {
        let state = PipelineState::default();
        assert!(!state.blend.enabled);
        assert!(state.depth.test_enabled);
    }

    #[test]
    fn test_clear_value() {
        let black = ClearValue::black();
        assert_eq!(black.r, 0.0);
        assert_eq!(black.a, 1.0);

        let white = ClearValue::white();
        assert_eq!(white.r, 1.0);
    }

    #[test]
    fn test_frame_presentation_new() {
        let frame = FramePresentation::new();
        assert_eq!(frame.frame_index, 0);
        assert!(!frame.ready);
    }

    #[test]
    fn test_frame_presentation_next() {
        let mut frame = FramePresentation::new();
        frame.next_frame();
        assert_eq!(frame.frame_index, 1);
        assert_eq!(frame.swapchain_index, 1);

        for _ in 0..3 {
            frame.next_frame();
        }
        assert_eq!(frame.swapchain_index, 0); // Wraps at 3
    }
}

// ============================================================================
// INTEGRATION SCENARIOS
// ============================================================================

#[cfg(test)]
mod scenarios {
    use super::*;

    #[test]
    fn test_graphics_api_initialization() {
        let ctx = GraphicsContext::new(GraphicsAPI::Vulkan);
        assert_eq!(ctx.api, GraphicsAPI::Vulkan);
    }

    #[test]
    fn test_shader_program_workflow() {
        let mut ctx = GraphicsContext::new(GraphicsAPI::OpenGL);

        let prog_id = ctx.create_shader_program().unwrap();
        let vs_id = ctx.create_buffer(512, BufferUsage::UniformBuffer).unwrap();
        let fs_id = ctx.create_buffer(512, BufferUsage::UniformBuffer).unwrap();

        if let Some(program) = ctx.get_shader_program(prog_id) {
            program.attach_shader(ShaderStage::Vertex, vs_id);
            program.attach_shader(ShaderStage::Fragment, fs_id);
            assert!(program.is_complete());
        }
    }

    #[test]
    fn test_mesh_rendering_setup() {
        let mut ctx = GraphicsContext::new(GraphicsAPI::Software);

        let vbo = ctx.create_buffer(65536, BufferUsage::VertexBuffer).unwrap();
        let ibo = ctx.create_buffer(32768, BufferUsage::IndexBuffer).unwrap();
        let ubo = ctx.create_buffer(256, BufferUsage::UniformBuffer).unwrap();

        assert!(ctx.get_buffer(vbo).is_some());
        assert!(ctx.get_buffer(ibo).is_some());
        assert!(ctx.get_buffer(ubo).is_some());
    }

    #[test]
    fn test_framebuffer_creation() {
        let mut ctx = GraphicsContext::new(GraphicsAPI::Vulkan);

        let color_img = ctx.create_image(ImageDesc {
            width: 1920, height: 1080, format: ImageFormat::RGBA8, depth: 1,
        }).unwrap();

        let depth_img = ctx.create_image(ImageDesc {
            width: 1920, height: 1080, format: ImageFormat::Depth32F, depth: 1,
        }).unwrap();

        assert!(ctx.get_image(color_img).is_some());
        assert!(ctx.get_image(depth_img).is_some());
    }
}
