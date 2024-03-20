// include wasm component model interface for webgpu
// export wasm comenent model for the guest application
wit_bindgen::generate!({
    path: "../../wit",
    world: "component:webgpu/example",
    exports: {
        world: ExampleCompute,
    },
});

struct ExampleCompute;

const MAP_READ: u32 = 1 << 0;
const STORAGE: u32 = 1 << 7;
const COPY_DST: u32 = 1 << 3;
const COPY_SRC: u32 = 1 << 2;


impl Guest for ExampleCompute {
    fn start() {
        compute();
    }
}

use component::webgpu::{
    webgpu,
};

// simple comute shader to sum two vectors
const SHADER_CODE: &str = r#"
@group(0) @binding(0)
var<storage, read_write> result : array<f32>;
@group(0) @binding(1)
var<storage, read> input : array<f32>;

@compute @workgroup_size(1)
fn main(@builtin(global_invocation_id) gid : vec3<u32>) {
    result[gid.x] = result[gid.x]+input[gid.x];
}
"#;



fn f32_array_to_bytes(float_array: &[f32]) -> Vec<u8> {
    let mut byte_vec: Vec<u8> = Vec::new();
    for &value in float_array {
        byte_vec.extend_from_slice(&value.to_le_bytes());
    }
    byte_vec
}

fn host_add_vec(res: &mut [f32], input: &[f32]){
    print!("len of data:{:?}\n", res.len());
    for i in 0..res.len()-1{
        res[i] = res[i]+input[i]
    }
}

fn compute() {
    print("setup compute pipeline!\n");
    // request a device
    let gpu = webgpu::get_gpu();
    let adapter = gpu.request_adapter(None);
    let device = adapter.request_device(None);

    // create a command encoder to define a pipeline
    let encoder = device.create_command_encoder(None);
    print("encoder created!\n");
    const N: usize = 2^10;
    // define input 
    let mut float_array1: [f32; N] = [3.14; N];
    let float_array2: [f32; N] = [3.14; N];
    let slice_size = float_array1.len() * std::mem::size_of::<f32>();
    let size = slice_size as webgpu::GpuSize64;
    
    // define our target buffer , it suppors map read and write Destination flags
    let staging_buffer = device.create_buffer(&webgpu::GpuBufferDescriptor {
        label: None,
        size,
        usage:  MAP_READ | COPY_DST,
        mapped_at_creation: Some(false),
        context: None,
    });
    print("staging buffer created!\n");
    
    // define our device buffer
    let storage_buffer = device.create_buffer(&&webgpu::GpuBufferDescriptor {
        label: None,
        size,
        usage: STORAGE
            |  COPY_DST
            |  COPY_SRC,
        mapped_at_creation: Some(true),
        context: Some(f32_array_to_bytes(&float_array1)),
    });

    let storage_buffer1 = device.create_buffer(&&webgpu::GpuBufferDescriptor {
        label: None,
        size,
        usage: STORAGE
            |  COPY_DST
            |  COPY_SRC,
        mapped_at_creation: Some(true),
        context: Some(f32_array_to_bytes(&float_array2)),
    });
    print("storage buffer created!\n");
    // setup a descriptor for a compute pipeline 
    // it includes compute shader code and shader entry point
    let pipeline_description = webgpu::GpuComputePipelineDescriptor {
        compute: webgpu::GpuProgrammableStage {
            module: &device.create_shader_module(webgpu::GpuShaderModuleDescriptor {
                code: SHADER_CODE.to_string(),
                label: None,
                compilation_hints: None,
            }),
            entry_point: Some("main".to_string()),
        }
    };
    print("pipeline desc created!\n");

    // create a compute pipeline
    let compute_pipeline = device.create_compute_pipeline(pipeline_description);
    
    // setup a descriptor to bind inputs to the pipeline
    let bind_group_descriptor = webgpu::GpuBindGroupDescriptor{
        layout: compute_pipeline.get_bind_group_layout(0),
        label: None,
        entries : vec![
            webgpu::GpuBindGroupEntry{ 
                binding: 0,
                resource:  webgpu::GpuBindingResource::GpuBufferBinding(
                    webgpu::GpuBufferBinding{
                        buffer: storage_buffer.clone(),
                        offset: Some(0),
                        size: Some(size)},
                ),
            },
            webgpu::GpuBindGroupEntry{ 
                binding: 1,
                resource:  webgpu::GpuBindingResource::GpuBufferBinding(
                    webgpu::GpuBufferBinding{
                        buffer: storage_buffer1.clone(),
                        offset: Some(0),
                        size: Some(size)},
                ),
            }
            ],
    };

    // define a bind group 
    let bind_group = device.create_bind_group(bind_group_descriptor);
    print("bind group created!\n");

    // setup a compute pass including pipeline, bind group a dispatch configuration
    let compute_pass = encoder.begin_compute_pass(None);
    compute_pass.set_pipeline(&compute_pipeline);
    compute_pass.set_bind_group(0, &bind_group, None);

    // configure 4 workgroups for the execution
    compute_pass.dispatch_workgroups(float_array1.len().try_into().unwrap(), Some(1), Some(1));
    webgpu::GpuComputePassEncoder::end(compute_pass, &encoder);
    print("compute pass create!\n");

     // setup a stage to copy device to target buffer
    encoder.copy_buffer_to_buffer(
        storage_buffer.clone(),
        0,
        staging_buffer.clone(),
        0,
        size,
    );
    print("copy setup done!\n");
    let timer = webgpu::new_timer();
    // submit all command to the device queue
    timer.tick();
    device
       .queue()
       .submit(vec![webgpu::GpuCommandEncoder::finish(encoder, None)]);
    
    
    // perform read mapping on staging target
    staging_buffer.map_async(2,Some(0),Some(size));
    print("mapped device to host buffer!\n");
    timer.tock();

    timer.tick();
    host_add_vec(&mut float_array1, &float_array2);
    timer.tock();
}
