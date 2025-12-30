use anyhow::Result;
use rayos_kernel::system1::ray_logic::LogicBVHBuilder;

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Signal to the library that this run is meant for RT verification.
    std::env::set_var("RAYOS_RT_CORE", "1");
    std::env::set_var("RAYOS_RT_CORE_SMOKE", "1");
    std::env::set_var("RAYOS_RT_CORE_LOG", "1");

    log::info!("RayOS: RT smoke: start");

    // 1) Prove we can create a Vulkan device with RT features (if supported).
    match rayos_kernel::hal::rt_vulkan::self_test_create_rt_device() {
        Ok(()) => {
            log::info!("RayOS: RT smoke: create_rt_device=OK");
        }
        Err(e) => {
            log::warn!("RayOS: RT smoke: create_rt_device=UNSUPPORTED ({e})");
            if std::env::var_os("RAYOS_RT_REQUIRED").is_some() {
                anyhow::bail!("RT required but unsupported: {e}");
            }
        }
    }

    // 2) Run the rayQuery branch self-test for both outcomes.
    let hit = rayos_kernel::hal::rt_vulkan::eval_threshold_branch(0.9).map_err(|e| {
        if std::env::var_os("RAYOS_RT_REQUIRED").is_some() {
            e
        } else {
            log::warn!("RayOS: RT smoke: rayQuery hit test failed: {e}");
            e
        }
    });

    let miss = rayos_kernel::hal::rt_vulkan::eval_threshold_branch(0.1).map_err(|e| {
        if std::env::var_os("RAYOS_RT_REQUIRED").is_some() {
            e
        } else {
            log::warn!("RayOS: RT smoke: rayQuery miss test failed: {e}");
            e
        }
    });

    if let (Ok(true), Ok(false)) = (hit, miss) {
        log::info!("RayOS: RT smoke: rayQuery branch tests=OK");
    } else if std::env::var_os("RAYOS_RT_REQUIRED").is_some() {
        anyhow::bail!("RT required but rayQuery branch tests failed");
    } else {
        log::warn!("RayOS: RT smoke: rayQuery branch tests=SKIPPED/FAILED (non-fatal)");
    }

    // 3) Prove the LogicBVH traversal can route through the RT-core path.
    let mut builder = LogicBVHBuilder::new();
    let bvh = builder.build_switch(vec![0, 1], vec![100, 200], 999);

    let a = bvh.trace(&[0.9, 0.1]);
    let b = bvh.trace(&[0.1, 0.9]);

    if a != 100 || b != 200 {
        anyhow::bail!("unexpected BVH results: got a={a} b={b}");
    }

    log::info!("RayOS: RT smoke: LogicBVH traversal=OK");
    log::info!("RayOS: RT smoke: done");

    Ok(())
}
