use std::sync::{Arc, Weak};

use anyhow::Result;
use mlua::{FromLua, Lua, UserData, Value};
use raug::graph::NodeIndex;
use raug_server::graph::{GraphOp, NameOrIndex};

use crate::client::Client;

async fn binary_op(
    op: &str,
    client: Arc<Client>,
    lhs: NodeIndex,
    lhs_output: NameOrIndex,
    rhs: NodeIndex,
    rhs_output: NameOrIndex,
) -> Result<LuaNode> {
    let resp = client
        .request(GraphOp::AddProcessor {
            name: op.to_string(),
        })
        .await?;

    let target = *resp.as_node_index().unwrap();

    let op0 = GraphOp::Connect {
        source: lhs,
        source_output: lhs_output,
        target,
        target_input: NameOrIndex::Index(0),
    };
    let op1 = GraphOp::Connect {
        source: rhs,
        source_output: rhs_output,
        target,
        target_input: NameOrIndex::Index(1),
    };
    tokio::try_join!(client.request(op0), client.request(op1))?;

    Ok(LuaNode {
        client: Arc::downgrade(&client),
        index: target,
    })
}

async fn value_to_output(
    _lua: &Lua,
    client: Arc<Client>,
    value: Value,
) -> Result<(NodeIndex, NameOrIndex)> {
    match value {
        Value::Integer(value) => {
            let value = value as f32;
            let node = client.request(GraphOp::AddConstantF32(value)).await?;
            let node = *node.as_node_index().unwrap();
            Ok((node, NameOrIndex::Index(0)))
        }
        Value::Number(value) => {
            let value = value as f32;
            let node = client.request(GraphOp::AddConstantF32(value)).await?;
            let node = *node.as_node_index().unwrap();
            Ok((node, NameOrIndex::Index(0)))
        }
        Value::UserData(value) => {
            if let Ok(value) = value.borrow::<LuaOutput>() {
                Ok((value.node, value.output.clone()))
            } else if let Ok(value) = value.borrow::<LuaNode>() {
                Ok((value.index, NameOrIndex::Index(0)))
            } else {
                Err(mlua::Error::runtime("Invalid rhs (userdata)").into())
            }
        }
        value => Err(mlua::Error::runtime(format!("Invalid rhs: {:?}", value)).into()),
    }
}

#[derive(Clone, FromLua)]
pub struct LuaNode {
    pub client: Weak<Client>,
    pub index: NodeIndex,
}

impl UserData for LuaNode {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method_mut(
            "replace",
            move |lua, mut this, replacement: Value| async move {
                let client = this.client.upgrade().unwrap();
                let (replacement, _) = value_to_output(&lua, client.clone(), replacement).await?;
                let node = client
                    .request(GraphOp::ReplaceNode {
                        target: this.index,
                        replacement,
                    })
                    .await?;
                let node = *node.as_node_index().unwrap();
                this.index = node;
                Ok(LuaNode {
                    client: Arc::downgrade(&client),
                    index: node,
                })
            },
        );

        methods.add_meta_method("__index", move |_lua, this, key: Value| match key {
            Value::Integer(v) => Ok(LuaOutput {
                client: this.client.clone(),
                node: this.index,
                output: NameOrIndex::Index(v as u32),
            }),
            Value::String(v) => Ok(LuaOutput {
                client: this.client.clone(),
                node: this.index,
                output: NameOrIndex::Name(v.to_string_lossy()),
            }),
            _ => Err(mlua::Error::runtime("Invalid index")),
        });
        methods.add_async_meta_method("__add", move |lua, lhs, rhs: Value| async move {
            let client = lhs.client.upgrade().unwrap();
            let (rhs, rhs_output) = value_to_output(&lua, client.clone(), rhs).await?;
            let res = binary_op(
                "Add",
                client,
                lhs.index,
                NameOrIndex::Index(0),
                rhs,
                rhs_output,
            )
            .await?;
            Ok(res)
        });
        methods.add_async_meta_method("__sub", move |lua, lhs, rhs: Value| async move {
            let client = lhs.client.upgrade().unwrap();
            let (rhs, rhs_output) = value_to_output(&lua, client.clone(), rhs).await?;
            let res = binary_op(
                "Sub",
                client,
                lhs.index,
                NameOrIndex::Index(0),
                rhs,
                rhs_output,
            )
            .await?;
            Ok(res)
        });
        methods.add_async_meta_method("__mul", move |lua, lhs, rhs: Value| async move {
            let client = lhs.client.upgrade().unwrap();
            let (rhs, rhs_output) = value_to_output(&lua, client.clone(), rhs).await?;
            let res = binary_op(
                "Mul",
                client,
                lhs.index,
                NameOrIndex::Index(0),
                rhs,
                rhs_output,
            )
            .await?;
            Ok(res)
        });
        methods.add_async_meta_method("__div", move |lua, lhs, rhs: Value| async move {
            let client = lhs.client.upgrade().unwrap();
            let (rhs, rhs_output) = value_to_output(&lua, client.clone(), rhs).await?;
            let res = binary_op(
                "Div",
                client,
                lhs.index,
                NameOrIndex::Index(0),
                rhs,
                rhs_output,
            )
            .await?;
            Ok(res)
        });
    }
}

#[derive(Clone, FromLua)]
pub struct LuaOutput {
    pub client: Weak<Client>,
    pub node: NodeIndex,
    pub output: NameOrIndex,
}

impl UserData for LuaOutput {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_meta_method("__add", move |lua, lhs, rhs: Value| async move {
            let client = lhs.client.upgrade().unwrap();
            let (rhs, rhs_output) = value_to_output(&lua, client.clone(), rhs).await?;
            let res =
                binary_op("Add", client, lhs.node, lhs.output.clone(), rhs, rhs_output).await?;
            Ok(res)
        });
        methods.add_async_meta_method("__sub", move |lua, lhs, rhs: Value| async move {
            let client = lhs.client.upgrade().unwrap();
            let (rhs, rhs_output) = value_to_output(&lua, client.clone(), rhs).await?;
            let res =
                binary_op("Sub", client, lhs.node, lhs.output.clone(), rhs, rhs_output).await?;
            Ok(res)
        });
        methods.add_async_meta_method("__mul", move |lua, lhs, rhs: Value| async move {
            let client = lhs.client.upgrade().unwrap();
            let (rhs, rhs_output) = value_to_output(&lua, client.clone(), rhs).await?;
            let res =
                binary_op("Mul", client, lhs.node, lhs.output.clone(), rhs, rhs_output).await?;
            Ok(res)
        });
        methods.add_async_meta_method("__div", move |lua, lhs, rhs: Value| async move {
            let client = lhs.client.upgrade().unwrap();
            let (rhs, rhs_output) = value_to_output(&lua, client.clone(), rhs).await?;
            let res =
                binary_op("Div", client, lhs.node, lhs.output.clone(), rhs, rhs_output).await?;
            Ok(res)
        });
    }
}
