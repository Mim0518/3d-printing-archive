import { Center, Loader } from '@mantine/core'
import type { ReactNode } from 'react'
import { Navigate, Route, Routes, useLocation } from 'react-router'

import Marco from './componentes/Marco'
import Catalogo from './paginas/Catalogo'
import Inicio from './paginas/Inicio'
import Login from './paginas/Login'
import Modelo from './paginas/Modelo'
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
        element={
          <ConSesion>
            <Marco />
          </ConSesion>
        }
      >
        <Route index element={<Inicio />} />
        <Route path="catalogo" element={<Catalogo />} />
        <Route path="catalogo/:id" element={<Modelo />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Route>
    </Routes>
  )
}
