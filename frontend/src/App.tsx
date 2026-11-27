import { Center, Loader } from '@mantine/core'
import type { ReactNode } from 'react'
import { Navigate, Route, Routes, useLocation } from 'react-router'

import Marco from './componentes/Marco'
import Catalogo from './paginas/Catalogo'
import Etiquetas from './paginas/Etiquetas'
import Coleccion from './paginas/Coleccion'
import Existencias from './paginas/Existencias'
import HojaColeccion from './paginas/HojaColeccion'
import Importar from './paginas/Importar'
import Impresora from './paginas/Impresora'
import Inicio from './paginas/Inicio'
import Login from './paginas/Login'
import Modelo from './paginas/Modelo'
import Mudanza from './paginas/Mudanza'
import Mudanzas from './paginas/Mudanzas'
import Pedido from './paginas/Pedido'
import Perfil from './paginas/Perfil'
import Placa from './paginas/Placa'
import Placas from './paginas/Placas'
import Taller from './paginas/Taller'
import Ventas from './paginas/Ventas'
import { useYo } from './sesion'

function ConSesion({ children }: { children: ReactNode }) {
  const { data: usuario, isPending } = useYo()
  const ubicacion = useLocation()
  if (isPending) {
    return (
      <Center h="100vh">
        <Loader />
      </Center>
    )
  }
  if (!usuario) return <Navigate to="/login" replace state={{ desde: ubicacion.pathname }} />
  return children
}

export default function App() {
  return (
    <Routes>
      <Route path="/login" element={<Login />} />
      <Route
        path="/hoja/coleccion/:id"
        element={
          <ConSesion>
            <HojaColeccion />
          </ConSesion>
        }
      />
      <Route
        element={
          <ConSesion>
            <Marco />
          </ConSesion>
        }
      >
        <Route index element={<Inicio />} />
        <Route path="catalogo" element={<Catalogo />} />
        <Route path="catalogo/:id" element={<Modelo />} />
        <Route path="etiquetas" element={<Etiquetas />} />
        <Route path="mudanzas" element={<Mudanzas />} />
        <Route path="mudanzas/:id" element={<Mudanza />} />
        <Route path="placas" element={<Placas />} />
        <Route path="placas/:id" element={<Placa />} />
        <Route path="impresoras" element={<Taller />} />
        <Route path="impresoras/importar" element={<Importar />} />
        <Route path="impresoras/:id" element={<Impresora />} />
        <Route path="perfiles/:id" element={<Perfil />} />
        <Route path="existencias" element={<Existencias />} />
        <Route path="ventas" element={<Ventas />} />
        <Route path="ventas/pedidos/:id" element={<Pedido />} />
        <Route path="ventas/colecciones/:id" element={<Coleccion />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Route>
    </Routes>
  )
}
