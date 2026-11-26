// Visor 3D (three.js). Se carga aparte para que three.js no pese en el resto de la app.
import { Bounds, Center, Html, OrbitControls, useProgress } from '@react-three/drei'
import { Canvas, useLoader } from '@react-three/fiber'
import { Component, type ReactNode, Suspense, useEffect } from 'react'
import * as THREE from 'three'
import { ThreeMFLoader } from 'three/examples/jsm/loaders/3MFLoader.js'
import { OBJLoader } from 'three/examples/jsm/loaders/OBJLoader.js'
import { PLYLoader } from 'three/examples/jsm/loaders/PLYLoader.js'
import { STLLoader } from 'three/examples/jsm/loaders/STLLoader.js'

const MATERIAL = new THREE.MeshStandardMaterial({ color: '#c8c8cf', roughness: 0.55, metalness: 0.05 })

function Geometria({ url, ext }: { url: string; ext: 'stl' | 'ply' }) {
  const geo = useLoader(ext === 'stl' ? STLLoader : PLYLoader, url)
  if (!geo.getAttribute('normal')) geo.computeVertexNormals()
  return <mesh geometry={geo} material={MATERIAL} />
}

function Objeto({ url, ext }: { url: string; ext: 'obj' | '3mf' }) {
  const obj = useLoader(ext === 'obj' ? OBJLoader : ThreeMFLoader, url)
  useEffect(() => {
    obj.traverse((o) => {
      if (o instanceof THREE.Mesh) o.material = MATERIAL
    })
  }, [obj])
  return <primitive object={obj} />
}

function Cargando() {
  const { progress } = useProgress()
  return (
    <Html center>
      <div style={{ color: 'var(--mantine-color-dimmed)', whiteSpace: 'nowrap' }}>Cargando… {Math.round(progress)} %</div>
    </Html>
  )
}

class Atrapa extends Component<{ children: ReactNode }, { error: string | null }> {
  state = { error: null as string | null }
  static getDerivedStateFromError(e: unknown) {
    return { error: e instanceof Error ? e.message : String(e) }
  }
  render() {
    if (this.state.error) {
      return (
        <div style={{ padding: 24, color: 'var(--mantine-color-red-5)' }}>
          {/webgl/i.test(this.state.error)
            ? 'Este navegador no tiene WebGL (gráficos 3D), así que no puede mostrar la malla. Ábrela en otro navegador o descárgala.'
            : `No se pudo abrir el archivo: ${this.state.error}`}
        </div>
      )
    }
    return this.props.children
  }
}

export default function Visor3D({ archivoId, ext }: { archivoId: number; ext: string }) {
  const url = `/api/v1/archivos/${archivoId}/contenido`
  const e = ext.toLowerCase()
  const malla =
    e === 'stl' || e === 'ply' ? <Geometria url={url} ext={e} /> : e === 'obj' || e === '3mf' ? <Objeto url={url} ext={e} /> : null
  if (!malla) return <div style={{ padding: 24 }}>El visor no abre archivos .{ext}.</div>
  return (
    <Atrapa>
      <div style={{ height: '70vh', borderRadius: 8, overflow: 'hidden', background: 'var(--mantine-color-dark-7)' }}>
        <Canvas camera={{ position: [-1.6, 1.1, 1.6], fov: 40 }} dpr={[1, 2]}>
          <ambientLight intensity={0.6} />
          <directionalLight position={[3, 5, 4]} intensity={1.6} />
          <directionalLight position={[-4, 2, -3]} intensity={0.5} />
          <Suspense fallback={<Cargando />}>
            <Bounds fit clip observe margin={1.15}>
              <Center>
                {/* las mallas vienen con Z hacia arriba; three.js usa Y */}
                <group rotation={[-Math.PI / 2, 0, 0]}>{malla}</group>
              </Center>
            </Bounds>
          </Suspense>
          <OrbitControls makeDefault />
        </Canvas>
      </div>
    </Atrapa>
  )
}
