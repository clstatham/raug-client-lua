use std::{
    net::SocketAddr,
    sync::{Arc, Weak},
    time::Duration,
};

use anyhow::Result;
use convert_case::{Case, Casing};
use mlua::*;
use raug::graph::NodeIndex;
use raug_server::graph::{GraphOp, GraphOpResponse, NameOrIndex};
use tokio::net::{ToSocketAddrs, UdpSocket};

use crate::graph::{LuaNode, LuaOutput};

pub struct Client {
    pub sref: Weak<Self>,
    pub lua: Lua,
    pub socket: Arc<UdpSocket>,
    pub remote_addr: SocketAddr,
}

impl Client {
    pub async fn bind(
        local_addr: impl ToSocketAddrs,
        remote_addr: SocketAddr,
    ) -> Result<Arc<Self>> {
        let socket = Arc::new(UdpSocket::bind(local_addr).await?);

        let lua = Lua::new();

        let this = Arc::new_cyclic(|sref| Self {
            sref: sref.clone(),
            socket,
            lua,
            remote_addr,
        });

        this.lua.globals().set(
            "play",
            this.lua.create_async_function({
                let client = this.clone();
                move |_lua, _: ()| {
                    let client = client.clone();
                    async move {
                        client.request(GraphOp::Play).await?;
                        Ok(())
                    }
                }
            })?,
        )?;

        this.lua.globals().set(
            "stop",
            this.lua.create_async_function({
                let client = this.clone();
                move |_lua, _: ()| {
                    let client = client.clone();
                    async move {
                        client.request(GraphOp::Stop).await?;
                        Ok(())
                    }
                }
            })?,
        )?;

        this.lua.globals().set(
            "sleep",
            this.lua.create_async_function({
                move |_lua, duration: f64| async move {
                    tokio::time::sleep(Duration::from_secs_f64(duration)).await;
                    Ok(())
                }
            })?,
        )?;

        this.lua.globals().set(
            "dac",
            this.lua.create_async_function({
                let client = this.clone();
                move |lua, args: Value| {
                    let client = client.clone();
                    async move {
                        let args = args.into_lua_multi(&lua)?;
                        let resp = client.request(GraphOp::AddDac).await?;
                        let dac = *resp.as_node_index().unwrap();

                        let res = client.connect_inputs_and_outputs(dac, args).await?;
                        Ok(res)
                    }
                }
            })?,
        )?;

        this.register_lua_procs(["SineOscillator", "BlSawOscillator"])?;

        Ok(this)
    }

    pub async fn request(&self, op: GraphOp) -> Result<GraphOpResponse> {
        op.request(&self.socket, self.remote_addr).await
    }

    fn register_lua_procs<'a>(&self, procs: impl IntoIterator<Item = &'a str>) -> Result<()> {
        for proc in procs.into_iter() {
            self.register_lua_proc(proc)?;
        }
        Ok(())
    }

    async fn connect_inputs_and_outputs(
        &self,
        node: NodeIndex,
        args: MultiValue,
    ) -> Result<LuaNode> {
        for (target_input, arg) in args.iter().enumerate() {
            let (source, source_output) = match arg {
                Value::Nil => {
                    continue;
                }
                Value::Number(n) => {
                    let op = GraphOp::AddConstantF32(*n as f32);
                    let resp = self.request(op).await?;
                    let node_index = *resp.as_node_index().unwrap();
                    (node_index, NameOrIndex::Index(0))
                }
                Value::Integer(n) => {
                    let op = GraphOp::AddConstantF32(*n as f32);
                    let resp = self.request(op).await?;
                    let node_index = *resp.as_node_index().unwrap();
                    (node_index, NameOrIndex::Index(0))
                }
                Value::UserData(data) => {
                    if let Ok(data) = data.borrow::<LuaOutput>() {
                        (data.node, data.output.clone())
                    } else if let Ok(data) = data.borrow::<LuaNode>() {
                        (data.index, NameOrIndex::Index(0))
                    } else {
                        return Err(Error::runtime("Invalid argument").into());
                    }
                }
                _ => return Err(Error::runtime("Invalid argument").into()),
            };

            let op = GraphOp::Connect {
                source,
                source_output,
                target: node,
                target_input: NameOrIndex::Index(target_input as u32),
            };

            let resp = self.request(op).await?;
            assert_eq!(resp, GraphOpResponse::None);
        }

        Ok(LuaNode {
            client: self.sref.clone(),
            index: node,
        })
    }

    fn register_lua_proc(&self, proc: &str) -> Result<()> {
        let proc = proc.to_string();
        self.lua.globals().set(
            proc.to_case(Case::Snake),
            self.lua.create_async_function({
                let socket = self.socket.clone();
                let remote_addr = self.remote_addr;
                let proc = proc.clone();
                let client = self.sref.clone().upgrade().unwrap();
                move |lua, args: Value| {
                    let socket = socket.clone();
                    let proc = proc.clone();
                    let client = client.clone();
                    async move {
                        let args = args.into_lua_multi(&lua)?;
                        let op = GraphOp::AddProcessor {
                            name: proc.to_case(Case::UpperCamel),
                        };
                        let resp = op.request(&socket, remote_addr).await?;
                        let target = *resp.as_node_index().unwrap();

                        let res = client.connect_inputs_and_outputs(target, args).await?;

                        Ok(res)
                    }
                }
            })?,
        )?;
        Ok(())
    }

    pub async fn exec(&self, chunk: impl AsChunk<'_>) -> Result<()> {
        self.lua.load(chunk).exec_async().await?;
        Ok(())
    }

    pub async fn eval<R: FromLua>(&self, chunk: impl AsChunk<'_>) -> Result<R> {
        let value = self.lua.load(chunk).eval_async().await?;
        Ok(value)
    }
}
