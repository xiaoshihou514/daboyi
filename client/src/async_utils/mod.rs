use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use std::future::Future;

/// 异步任务处理模块
pub struct AsyncUtilsPlugin;

impl Plugin for AsyncUtilsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AsyncTasks>();
    }
}

/// 异步任务管理资源
#[derive(Resource, Default)]
pub struct AsyncTasks {
    tasks: Vec<Task<()>>,
}

impl AsyncTasks {
    /// 提交异步任务
    pub fn spawn<F>(&mut self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let task_pool = AsyncComputeTaskPool::get();
        let task = task_pool.spawn(future);
        self.tasks.push(task);
    }

    /// 清理完成的任务
    pub fn cleanup(&mut self) {
        self.tasks.retain(|task| !task.is_finished());
    }
}

/// 在异步计算池中执行任务
pub fn spawn_async_compute<F, R>(f: F) -> impl Future<Output = R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let task_pool = AsyncComputeTaskPool::get();
    task_pool.spawn(async move { f() })
}
