use std::{
    net::SocketAddr,
    sync::{Arc, Weak},
    time::Duration,
};

use anyhow::Result;
use convert_case::{Case, Casing};
use mlua::*;
use raug_graph::graph::NodeIndex;
use raug_server::graph::{GraphOp, GraphOpResponse, NameOrIndex};
use tokio::net::{ToSocketAddrs, UdpSocket};

use crate::graph::{LuaMixer, LuaNode, value_to_output};

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
        socket.connect(remote_addr).await?;

        let lua = Lua::new();

        let this = Arc::new_cyclic(|sref| Self {
            sref: sref.clone(),
            socket,
            lua,
            remote_addr,
        });

        this.lua.globals().set(
            "mix",
            this.lua.create_userdata(LuaMixer {
                client: this.sref.clone(),
            })?,
        )?;

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

        this.register_lua_procs([
            "PhaseAccumulator",
            "SineOscillator",
            "BlSawOscillator",
            "PeakLimiter",
            "Metro",
            "Decay",
            "Adsr",
        ])?;

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
            let (source, source_output) =
                value_to_output(self.sref.upgrade().unwrap(), arg.clone()).await?;

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
                let proc = proc.clone();
                let client = self.sref.clone().upgrade().unwrap();
                move |_lua, args: MultiValue| {
                    let proc = proc.clone();
                    let client = client.clone();
                    async move {
                        let op = GraphOp::AddProcessor {
                            name: proc.to_case(Case::UpperCamel),
                        };
                        let resp = client.request(op).await?;
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
        let _ = self.eval::<Value>(chunk).await?;
        Ok(())
    }

    pub async fn eval<R: FromLuaMulti>(&self, chunk: impl AsChunk<'_>) -> Result<R> {
        let res = self.lua.load(chunk).eval_async().await?;
        Ok(R::from_lua_multi(res, &self.lua)?)
    }
}
