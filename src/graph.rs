use std::sync::{Arc, Weak};

use anyhow::Result;
use mlua::{FromLua, UserData, UserDataRef, Value};
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

    let op = GraphOp::Connect {
        source: lhs,
        source_output: lhs_output,
        target,
        target_input: NameOrIndex::Index(0),
    };
    client.request(op).await?;

    let op = GraphOp::Connect {
        source: rhs,
        source_output: rhs_output,
        target,
        target_input: NameOrIndex::Index(1),
    };
    client.request(op).await?;

    Ok(LuaNode {
        client: Arc::downgrade(&client),
        index: target,
    })
}

async fn value_to_output(client: Arc<Client>, value: Value) -> Result<(NodeIndex, NameOrIndex)> {
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
                Err(mlua::Error::runtime("Invalid rhs").into())
            }
        }
        _ => Err(mlua::Error::runtime("Invalid rhs").into()),
    }
}

#[derive(Clone, FromLua)]
pub struct LuaNode {
    pub client: Weak<Client>,
    pub index: NodeIndex,
}

impl UserData for LuaNode {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method("__index", move |lua, this, key: Value| match key {
            Value::Integer(v) => Ok(lua.create_userdata(LuaOutput {
                client: this.client.clone(),
                node: this.index,
                output: NameOrIndex::Index(v as u32),
            })),
            Value::String(v) => Ok(lua.create_userdata(LuaOutput {
                client: this.client.clone(),
                node: this.index,
                output: NameOrIndex::Name(v.to_string_lossy()),
            })),
            _ => Err(mlua::Error::runtime("Invalid index")),
        });
        methods.add_async_meta_method("__add", move |_lua, lhs, rhs: Value| async move {
            let client = lhs.client.upgrade().unwrap();
            let (rhs, rhs_output) = value_to_output(client.clone(), rhs).await?;
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
        methods.add_async_meta_method("__sub", move |_lua, lhs, rhs: Value| async move {
            let client = lhs.client.upgrade().unwrap();
            let (rhs, rhs_output) = value_to_output(client.clone(), rhs).await?;
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
        methods.add_async_meta_method("__mul", move |_lua, lhs, rhs: Value| async move {
            let client = lhs.client.upgrade().unwrap();
            let (rhs, rhs_output) = value_to_output(client.clone(), rhs).await?;
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
        methods.add_async_meta_method("__div", move |_lua, lhs, rhs: Value| async move {
            let client = lhs.client.upgrade().unwrap();
            let (rhs, rhs_output) = value_to_output(client.clone(), rhs).await?;
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
        methods.add_async_meta_method("__add", move |_lua, lhs, rhs: Value| async move {
            let client = lhs.client.upgrade().unwrap();
            let (rhs, rhs_output) = value_to_output(client.clone(), rhs).await?;
            let res =
                binary_op("Add", client, lhs.node, lhs.output.clone(), rhs, rhs_output).await?;
            Ok(res)
        });
        methods.add_async_meta_method("__sub", move |_lua, lhs, rhs: Value| async move {
            let client = lhs.client.upgrade().unwrap();
            let (rhs, rhs_output) = value_to_output(client.clone(), rhs).await?;
            let res =
                binary_op("Sub", client, lhs.node, lhs.output.clone(), rhs, rhs_output).await?;
            Ok(res)
        });
        methods.add_async_meta_method("__mul", move |_lua, lhs, rhs: Value| async move {
            let client = lhs.client.upgrade().unwrap();
            let (rhs, rhs_output) = value_to_output(client.clone(), rhs).await?;
            let res =
                binary_op("Mul", client, lhs.node, lhs.output.clone(), rhs, rhs_output).await?;
            Ok(res)
        });
        methods.add_async_meta_method("__div", move |_lua, lhs, rhs: Value| async move {
            let client = lhs.client.upgrade().unwrap();
            let (rhs, rhs_output) = value_to_output(client.clone(), rhs).await?;
            let res =
                binary_op("Div", client, lhs.node, lhs.output.clone(), rhs, rhs_output).await?;
            Ok(res)
        });
    }
}
