import { Alert, Button, Center, Paper, PasswordInput, Stack, Text, TextInput, Title } from '@mantine/core'
import { type FormEvent, useState } from 'react'
import { Navigate, useLocation, useNavigate } from 'react-router'

import { useLogin, useYo } from '../sesion'

export default function Login() {
  const { data: usuario } = useYo()
  const login = useLogin()
  const navegar = useNavigate()
  const ubicacion = useLocation()
  const [nombre, setNombre] = useState('')
  const [contrasena, setContrasena] = useState('')
  const destino = (ubicacion.state as { desde?: string } | null)?.desde ?? '/'

  if (usuario) return <Navigate to={destino} replace />

  function enviar(e: FormEvent) {
    e.preventDefault()
    login.mutate(
      { usuario: nombre, contrasena },
      { onSuccess: () => navegar(destino, { replace: true }) },
    )
  }

  return (
    <Center mih="100vh" p="md">
      <Paper component="form" onSubmit={enviar} withBorder shadow="sm" p="xl" w="100%" maw={380}>
        <Stack>
          <div>
            <Title order={2}>Archivo 3D</Title>
            <Text c="dimmed" size="sm">
              Catálogo, placas, existencias y ventas
            </Text>
          </div>
          {login.error && (
            <Alert color="red" variant="light">
              {login.error.message}
            </Alert>
          )}
          <TextInput
            label="Usuario"
            autoComplete="username"
            autoCapitalize="none"
            required
            value={nombre}
            onChange={(e) => setNombre(e.currentTarget.value)}
          />
          <PasswordInput
            label="Contraseña"
            autoComplete="current-password"
            required
            value={contrasena}
            onChange={(e) => setContrasena(e.currentTarget.value)}
          />
          <Button type="submit" loading={login.isPending}>
            Entrar
          </Button>
        </Stack>
      </Paper>
    </Center>
  )
}
