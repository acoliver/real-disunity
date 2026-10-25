import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {inflateSync} from 'node:zlib';

export function selectionExpression(fixture, profile) {
  return `(()=>{const f=document.querySelector('#fixture');f.value=${JSON.stringify(fixture)};f.dispatchEvent(new Event('change'));const p=document.querySelector('#profile');const desired=${JSON.stringify(profile || '')};if(!p.options.length||(desired&&![...p.options].some(o=>o.value===desired)))return null;p.value=desired;document.querySelector('#desktop').checked=false;return {fixture:f.value,profile:p.value,profiles:[...p.options].map(o=>({value:o.value,text:o.textContent})),desktop:document.querySelector('#desktop').checked};})()`;
}

export function nativeWorkerEvidence(maps) {
  const paths=maps.split('\n').map(line=>line.match(/^\S+\s+\S+\s+\S+\s+\S+\s+\d+\s+(\/.*)$/)?.[1]).filter(Boolean);
  const coreclr=paths.find(file=>file.endsWith('/libcoreclr.so'));
  const native=paths.find(file=>file.endsWith('/libworldrunner_disunity_ffi.so'));
  return coreclr&&native ? {coreclr,native} : null;
}

export function pngEvidence(bytes) {
  assert(bytes.subarray(0,8).equals(Buffer.from([137,80,78,71,13,10,26,10])), 'PNG signature');
  assert(bytes.length>=33, 'complete PNG header');
  const width=bytes.readUInt32BE(16), height=bytes.readUInt32BE(20);
  const chunks=[];
  let end=false;
  for(let offset=8;offset<bytes.length;) {
    assert(offset+12<=bytes.length, 'complete PNG chunk');
    const length=bytes.readUInt32BE(offset), type=bytes.toString('ascii',offset+4,offset+8);
    assert(offset+12+length<=bytes.length, 'complete PNG chunk');
    if(type==='IDAT')chunks.push(bytes.subarray(offset+8,offset+8+length));
    if(type==='IEND')end=true;
    offset+=12+length;
  }
  assert(end && chunks.length, 'PNG image and end chunks');
  const decoded=inflateSync(Buffer.concat(chunks));
  assert(decoded.length>height, 'decoded pixel data');
  return {sha256:createHash('sha256').update(bytes).digest('hex'),bytes:bytes.length,width,height,bit_depth:bytes[24],color_type:bytes[25],inflated_bytes:decoded.length};
}
