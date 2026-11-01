// The roads a face design has cut, each in its ramp's colour with its letter at both ends. A road shows with
// the face it's cut into, whatever is ticked among the strings, so a road can't be somewhere you can't see it.
// They give way to a section, which draws what's cut through the plane.

import * as THREE from 'three';
import type { ObjectSummary, ProjectSummary } from '../bridge';
import { rampColour } from './rampColours';
import type { Viewport } from './Viewport';

interface Label {
  element: HTMLElement;
  at: THREE.Vector3;
}

export class RoadLayer {
  private group = new THREE.Group();
  private labels: Label[] = [];
  private drawn = '';
  private wanted = true;

  constructor(private viewport: Viewport) {
    viewport.overlay.add(this.group);
    viewport.cameraMoved.add(this.place);
  }

  /** Brings the drawn roads in line with the project: those whose faces are showing. */
  sync(summary: ProjectSummary): void {
    const layerOn = (o: ObjectSummary) => summary.layers.find((l) => l.id === o.layer)?.visible ?? true;
    const faces = new Set(summary.objects.filter((o) => o.drawn && !o.hidden && layerOn(o)).map((o) => o.name));
    const edges = summary.objects.filter((o) => typeof o.attributes.road === 'string' && faces.has(String(o.attributes.road_face)) && o.vertices_local);
    const drawn = edges.map((o) => `${o.id}:${o.revision}`).join();
    if (drawn === this.drawn) return;
    this.drawn = drawn;
    this.clear();
    for (const edge of edges) {
      const name = String(edge.attributes.road);
      const points = (edge.vertices_local ?? []).map((p) => new THREE.Vector3(...p));
      if (points.length < 2) continue;
      const geometry = new THREE.BufferGeometry().setFromPoints(points.slice(1).flatMap((p, k) => [points[k], p]));
      const line = new THREE.LineSegments(geometry, new THREE.LineBasicMaterial({ color: rampColour(name), depthTest: false }));
      line.renderOrder = 9;
      line.frustumCulled = false;
      this.group.add(line);
      if (edge.name.endsWith(' pit edge')) this.labels.push(this.label(name, points[0]), this.label(name, points[points.length - 1]));
    }
    this.place();
  }

  /** Roads give way while a section is open. */
  setShown(shown: boolean): void {
    this.wanted = shown;
    this.group.visible = shown;
    this.place();
  }

  private label(name: string, at: THREE.Vector3): Label {
    const element = Object.assign(document.createElement('div'), { className: 'road-label', textContent: name });
    element.style.background = rampColour(name);
    this.viewport.renderer.domElement.parentElement?.append(element);
    return { element, at };
  }

  private clear(): void {
    for (const child of [...this.group.children]) {
      const line = child as THREE.LineSegments;
      line.geometry.dispose();
      (line.material as THREE.Material).dispose();
      this.group.remove(line);
    }
    for (const { element } of this.labels) element.remove();
    this.labels = [];
  }

  /** Holds each letter over its point on the screen. */
  private place = (): void => {
    const { clientWidth: width, clientHeight: height } = this.viewport.renderer.domElement;
    for (const { element, at } of this.labels) {
      const p = at.clone().project(this.viewport.camera);
      const inView = this.wanted && Math.abs(p.x) <= 1 && Math.abs(p.y) <= 1 && p.z < 1;
      element.hidden = !inView;
      if (inView) element.style.transform = `translate(${((p.x + 1) / 2) * width}px, ${((1 - p.y) / 2) * height}px) translate(-50%, -50%)`;
    }
  };
}
