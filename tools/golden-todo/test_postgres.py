#!/usr/bin/env python3
"""Shared gate DSN normalization, with no network or database substitutions."""
import unittest
from urllib.parse import parse_qs, unquote, urlsplit

from postgres import connection_parameters, postgres_uri


class ConnectionStrings(unittest.TestCase):
    def test_keyword_quoting_escapes_empty_values_and_duplicate_keys(self):
        value = r"host = '/tmp/a\\b\'c space' port=5544 user='a\'b' password='p\\w d' dbname=old application_name='' application_name='x y+z' sslmode=disable connect_timeout=3 options='-c search_path=public'"
        parameters = connection_parameters(value)
        self.assertEqual(parameters['host'], "/tmp/a\\b'c space")
        self.assertEqual(parameters['user'], "a'b")
        self.assertEqual(parameters['password'], 'p\\w d')
        self.assertEqual(parameters['application_name'], 'x y+z')
        result = urlsplit(postgres_uri(value, 'owned_database'))
        self.assertEqual(unquote(result.username), "a'b")
        self.assertEqual(unquote(result.password), 'p\\w d')
        self.assertEqual(result.port, 5544)
        self.assertEqual(result.path, '/owned_database')
        self.assertEqual(parse_qs(result.query), {
            'host': ["/tmp/a\\b'c space"], 'application_name': ['x y+z'],
            'sslmode': ['disable'], 'connect_timeout': ['3'], 'options': ['-c search_path=public'],
        })
        self.assertNotIn('+', result.query)  # libpq URI decoding is not form '+' decoding.

    def test_unquoted_escapes_and_empty_values_follow_libpq(self):
        self.assertEqual(connection_parameters(r'host=/tmp/a\ b user=a\ b dbname=x password='),
                         {'host': '/tmp/a b', 'user': 'a b', 'password': '', 'dbname': 'x'})
        self.assertEqual(connection_parameters("host=localhost password='' dbname=x")['password'], '')

    def test_uri_socket_and_common_parameters_survive_database_replacement(self):
        source = 'postgresql://name:p%27%5Cw@localhost:5544/old?host=%2Ftmp%2Fsock%20dir&sslmode=disable&application_name=a+b&options=-c%20timezone%3DUTC&dbname=override'
        result = urlsplit(postgres_uri(source, 'owned_database'))
        self.assertEqual(unquote(result.password), "p'\\w")
        self.assertEqual(result.path, '/owned_database')
        self.assertEqual(parse_qs(result.query), {
            'host': ['/tmp/sock dir'], 'sslmode': ['disable'],
            'application_name': ['a+b'], 'options': ['-c timezone=UTC'],
        })

    def test_network_ipv6_and_encoded_socket_authority(self):
        self.assertEqual(urlsplit(postgres_uri('postgres://u:p@[::1]:5544/old', 'new')).hostname, '::1')
        parsed = urlsplit(postgres_uri('postgresql://u@%2Ftmp%2Fsock:5544/old', 'new'))
        self.assertEqual(parse_qs(parsed.query)['host'], ['/tmp/sock'])

    def test_bad_quoting_refuses_without_echoing_values(self):
        for value in ["host='private-secret", 'host=private-secret\\', "host='private-secret'junk", '=private-secret', 'https://private-secret']:
            with self.assertRaises(ValueError) as error:
                postgres_uri(value, 'owned_database')
            self.assertNotIn('private-secret', str(error.exception))


if __name__ == '__main__':
    unittest.main()
