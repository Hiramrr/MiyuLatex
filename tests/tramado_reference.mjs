// Regenera la referencia con el bucle original de BetterThanEminus.
// node tests/tramado_reference.mjs ../BetterThanEminus/src/bg.js
import {readFileSync,writeFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
const source=readFileSync(process.argv[2]??'../BetterThanEminus/src/bg.js','utf8');
const bayer=source.slice(source.indexOf('const BAYER ='),source.indexOf('// ---------- color ----------'));
const loop=source.slice(source.indexOf('for (let y = 0',source.indexOf('const dither =')),source.indexOf('ctx.putImageData(data, 0, 0)'));
if(!bayer.includes('(v + 0.5) / 64')||!loop.includes('contrast'))throw Error('Cambió el código de referencia. Revisa la extracción.');
const calculate=new Function('px','w','h','br','bgG','bb','dither',`
${bayer}
const lum=(r,g,b)=>(0.2126*r+0.7152*g+0.0722*b)/255;
const smoothstep=(a,b,x)=>{const t=Math.min(1,Math.max(0,(x-a)/(b-a)));return t*t*(3-2*t);};
const bgL=lum(br,bgG,bb);
${loop}
return Array.from(px);`);
const width=17,height=19;
const input=Array.from({length:width*height*4},(_,i)=>i%4===3?255:(i*73+Math.floor(i/4)*19)%256);
const cases=[];
for(const base of [[22,19,31],[253,247,250]])for(const plain of [false,true]){
const expected=calculate(new Uint8ClampedArray(input),width,height,...base,!plain);
cases.push({base,plain,expected});
}
writeFileSync(new URL('tramado_reference.json',import.meta.url),JSON.stringify({source_sha256:createHash('sha256').update(source).digest('hex'),width,height,input,cases})+'\n');
console.log(`Referencia guardada: ${width*height*cases.length} píxeles del bucle original`);
