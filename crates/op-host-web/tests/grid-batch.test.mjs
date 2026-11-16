import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import test from 'node:test';
const source=await readFile(new URL('../src/op_ck_grid.js',import.meta.url),'utf8');
const {paintDots}=await import(`data:text/javascript;base64,${Buffer.from(source).toString('base64')}`);
test('thousands of round dots use one draw with the exact coordinates and diameter',()=>{
 const calls=[];let deleted=0,width,cap;
 const CK={Paint:class{setAntiAlias(){}setColor(){}setStyle(){}setStrokeCap(v){cap=v;}setStrokeWidth(v){width=v;}delete(){deleted++;}},Color4f:(...v)=>v,PaintStyle:{Stroke:1},StrokeCap:{Round:2},PointMode:{Points:3}};
 const points=new Float32Array(16000);const canvas={drawPoints(...args){calls.push(args);}};
 paintDots(CK,canvas,points,.75,[1,1,1,.18]);
 assert.equal(calls.length,1);assert.equal(calls[0][1],points);assert.equal(width,1.5);assert.equal(cap,2);assert.equal(deleted,1);
 paintDots(CK,canvas,[],.75,[1,1,1,1]);assert.equal(calls.length,1);
});
