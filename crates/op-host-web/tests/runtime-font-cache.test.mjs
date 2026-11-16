import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import test from 'node:test';
const source=await readFile(new URL('../src/op_ck_runtime_fonts.js',import.meta.url),'utf8');
const {runtimeFontFactory}=await import(`data:text/javascript;base64,${Buffer.from(source).toString('base64')}`);
function harness(){
 const stats={builds:0,deletes:0};
 const CK={TypefaceFontProvider:{Make:()=>({registerFont(){}})},Typeface:{MakeFreeTypeFaceFromData:()=>({})},
  ParagraphStyle:class{constructor(v){Object.assign(this,v);}},TextStyle:class{constructor(v){Object.assign(this,v);}},Color4f:(...v)=>v,FontWidth:{Normal:1},FontSlant:{Italic:1,Upright:0},
  ParagraphBuilder:{MakeFromFontProvider:()=>({addText(){},delete(){},build(){stats.builds++;return {layout(){},getLongestLine:()=>42,getAlphabeticBaseline:()=>12,getHeight:()=>16,delete(){stats.deletes++;}};}})}};
 const fonts=runtimeFontFactory()(CK);fonts.register(new Uint8Array([1]),JSON.stringify({family:'Inter',sha256:'a',weightMin:100,weightMax:900,variable:true}));
 return {fonts,stats,entry:fonts.resolve('inter',400,false)};
}
test('a page with more than 256 text runs remains warm across repaints',()=>{
 const {entry,stats}=harness();for(let frame=0;frame<3;frame++)for(let i=0;i<400;i++)entry.measure('Текст '+i,16);
 assert.equal(stats.builds,400);assert.equal(stats.deletes,0);
});
test('eviction deletes one oldest paragraph and retains recently used entries',()=>{
 const {entry,stats}=harness();for(let i=0;i<1024;i++)entry.measure('text'+i,16);
 entry.measure('text0',16);entry.measure('new',16);assert.equal(stats.deletes,1);
 const builds=stats.builds;entry.measure('text0',16);assert.equal(stats.builds,builds);
 entry.measure('text1',16);assert.equal(stats.builds,builds+1);
});
test('font registration invalidates every old paragraph',()=>{
 const {fonts,entry,stats}=harness();entry.measure('text',16);
 fonts.register(new Uint8Array([2]),JSON.stringify({family:'Noto Sans',sha256:'b',weightMin:400,weightMax:400}));
 assert.equal(stats.deletes,1);entry.measure('text',16);assert.equal(stats.builds,2);
});
